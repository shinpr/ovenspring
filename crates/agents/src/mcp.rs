//! The MCP servers the owner allows a role to use, resolved by name.
//!
//! A name is looked up, in order, in the repository's `.mcp.json` (shared by the team), in the
//! owner's Claude Code configuration (`~/.claude.json`, this repository's entry first), and in the
//! owner's Codex configuration (`$CODEX_HOME/config.toml`). Each CLI then receives exactly these.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub enum McpDef {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpServer {
    pub name: String,
    pub def: McpDef,
}

/// Where definitions are read from.
pub struct Sources<'a> {
    pub repo: &'a Path,
    pub home: &'a Path,
    pub codex_home: &'a Path,
}

/// One place servers are defined, in the order names are looked up.
struct Defined {
    /// Where the owner is told a server comes from.
    source: &'static str,
    /// Codex's configuration names an HTTP server's headers `http_headers`.
    codex: bool,
    servers: serde_json::Map<String, Value>,
}

fn defined(sources: &Sources<'_>) -> [Defined; 4] {
    let repo_json = read_json(&sources.repo.join(".mcp.json"));
    let claude_json = read_json(&sources.home.join(".claude.json"));
    let codex = std::fs::read_to_string(sources.codex_home.join("config.toml"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| serde_json::to_value(t).ok());
    let repo_key = sources.repo.display().to_string();
    let servers = |v: Option<&Value>| v.and_then(Value::as_object).cloned().unwrap_or_default();
    [
        Defined {
            source: "this repository's .mcp.json",
            codex: false,
            servers: servers(repo_json.as_ref().and_then(|v| v.get("mcpServers"))),
        },
        Defined {
            source: "your Claude Code",
            codex: false,
            servers: servers(
                claude_json
                    .as_ref()
                    .and_then(|v| v.get("projects"))
                    .and_then(|p| p.get(&repo_key))
                    .and_then(|p| p.get("mcpServers")),
            ),
        },
        Defined {
            source: "your Claude Code",
            codex: false,
            servers: servers(claude_json.as_ref().and_then(|v| v.get("mcpServers"))),
        },
        Defined {
            source: "your Codex",
            codex: true,
            servers: servers(codex.as_ref().and_then(|v| v.get("mcp_servers"))),
        },
    ]
}

/// Each name's definition, from the first source that defines it.
pub fn resolve(names: &[String], sources: &Sources<'_>) -> Result<Vec<McpServer>, String> {
    let defined = defined(sources);
    names
        .iter()
        .map(|name| {
            defined
                .iter()
                .find_map(|d| d.servers.get(name).map(|v| def_of(v, d.codex)))
                .flatten()
                .map(|def| McpServer {
                    name: name.clone(),
                    def,
                })
                .ok_or_else(|| {
                    format!(
                        "MCP server {name:?} is allowed but not defined with a command or URL in .mcp.json, ~/.claude.json or Codex config.toml"
                    )
                })
        })
        .collect()
}

/// An MCP server the owner can allow, where it is defined, and which of the factory's own
/// services (`linear`, `github`, `slack`) it reaches, judged from its name, command and address.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
pub struct Available {
    pub name: String,
    pub source: &'static str,
    pub reaches: Vec<&'static str>,
}

/// Every server the sources define, each once, from the first source that defines it.
pub fn available(sources: &Sources<'_>) -> Vec<Available> {
    let mut out: Vec<Available> = Vec::new();
    for d in defined(sources) {
        for (name, def) in &d.servers {
            if out.iter().any(|a| &a.name == name) {
                continue;
            }
            let said = format!("{name} {def}").to_lowercase();
            let reaches = ["linear", "github", "slack"]
                .into_iter()
                .filter(|service| said.contains(service))
                .collect();
            out.push(Available {
                name: name.clone(),
                source: d.source,
                reaches,
            });
        }
    }
    out
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn def_from_json(v: &Value) -> Option<McpDef> {
    if let Some(url) = v.get("url").and_then(Value::as_str) {
        return Some(McpDef::Http {
            url: url.to_string(),
            headers: string_map(v.get("headers")),
        });
    }
    let command = v.get("command").and_then(Value::as_str)?.to_string();
    Some(McpDef::Stdio {
        command,
        args: strings(v.get("args")),
        env: string_map(v.get("env")),
    })
}

fn def_of(v: &Value, codex: bool) -> Option<McpDef> {
    Some(match def_from_json(v)? {
        McpDef::Http { url, headers } if codex && headers.is_empty() => McpDef::Http {
            url,
            headers: string_map(v.get("http_headers")),
        },
        def @ (McpDef::Http { .. } | McpDef::Stdio { .. }) => def,
    })
}

/// `--mcp-config` content for Claude Code.
pub fn claude_config(servers: &[McpServer]) -> Value {
    let map: serde_json::Map<String, Value> = servers
        .iter()
        .map(|s| {
            let v = match &s.def {
                McpDef::Stdio { command, args, env } => {
                    json!({ "command": command, "args": args, "env": env })
                }
                McpDef::Http { url, headers } => {
                    json!({ "type": "http", "url": url, "headers": headers })
                }
            };
            (s.name.clone(), v)
        })
        .collect();
    json!({ "mcpServers": map })
}

/// The MCP servers OpenCode's own configuration defines for a launch in `cwd`: the owner's global
/// configuration and the repository's. OpenCode adds a launch's configuration to them, so each one
/// not allowed is switched off by name.
pub fn opencode_defined(cwd: &Path) -> Vec<String> {
    let global = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
        .map(|d| d.join("opencode"));
    let mut names = Vec::new();
    for dir in global.iter().map(std::path::PathBuf::as_path).chain([cwd]) {
        for file in ["opencode.json", "opencode.jsonc", "config.json"] {
            let Ok(text) = std::fs::read_to_string(dir.join(file)) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&without_comments(&text)) else {
                continue;
            };
            // Version 2 names servers under `mcp.servers`; version 1, and mixed files, directly
            // under `mcp`.
            if let Some(mcp) = v.get("mcp").and_then(Value::as_object) {
                for (key, entry) in mcp {
                    match (key.as_str(), entry.as_object()) {
                        ("servers", Some(servers)) => names.extend(servers.keys().cloned()),
                        (_, Some(_)) => names.push(key.clone()),
                        (_, None) => {}
                    }
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// JSONC as JSON: `//` and `/* */` comments outside strings removed.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                out.extend(chars.next());
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for n in chars.by_ref() {
                    if last == '*' && n == '/' {
                        break;
                    }
                    last = n;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// The `mcp` section of OpenCode's configuration: the allowed servers, and every other one
/// OpenCode's own configuration defines switched off. Version 1 names servers directly under
/// `mcp` and switches one off with `enabled: false`; version 2 names them under `mcp.servers` and
/// takes a typed stand-in with `disabled: true`. A value with `${NAME}` takes the variable in
/// OpenCode's own spelling, `{env:NAME}`.
pub fn opencode_config(servers: &[McpServer], defined: &[String], v2: bool) -> Value {
    // Version 2 drops an entry without a `type` and replaces a server's whole object by name, so
    // a server is switched off by a typed, disabled stand-in.
    let (on, off) = if v2 {
        (
            ("disabled", false),
            json!({ "type": "local", "command": ["true"], "disabled": true }),
        )
    } else {
        (("enabled", true), json!({ "enabled": false }))
    };
    let env_spelled = |values: &std::collections::BTreeMap<String, String>| -> Value {
        values
            .iter()
            .map(|(k, v)| (k.clone(), json!(opencode_env_refs(v))))
            .collect::<serde_json::Map<String, Value>>()
            .into()
    };
    let map: serde_json::Map<String, Value> = servers
        .iter()
        .map(|s| {
            let v = match &s.def {
                McpDef::Stdio { command, args, env } => {
                    let mut cmd = vec![command.clone()];
                    cmd.extend(args.iter().cloned());
                    json!({ "type": "local", "command": cmd, "environment": env_spelled(env), on.0: on.1 })
                }
                McpDef::Http { url, headers } => {
                    json!({ "type": "remote", "url": url, "headers": env_spelled(headers), on.0: on.1 })
                }
            };
            (s.name.clone(), v)
        })
        .collect();
    let mut map = map;
    for name in defined {
        if !map.contains_key(name) {
            map.insert(name.clone(), off.clone());
        }
    }
    if v2 {
        json!({ "servers": map })
    } else {
        Value::Object(map)
    }
}

/// `${NAME}` written as OpenCode's `{env:NAME}`.
fn opencode_env_refs(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start + 2..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str("{env:");
        out.push_str(&rest[start + 2..start + 2 + len]);
        out.push('}');
        rest = &rest[start + 3 + len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_takes_a_variable_in_its_own_spelling() {
        assert_eq!(
            opencode_env_refs("Bearer ${DOCS_TOKEN}"),
            "Bearer {env:DOCS_TOKEN}"
        );
        assert_eq!(opencode_env_refs("plain"), "plain");
    }

    #[test]
    fn a_name_resolves_from_the_repository_first_then_the_owners_configs() {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let codex = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join(".mcp.json"),
            r#"{"mcpServers":{"playwright":{"command":"npx","args":["@playwright/mcp","--headless"]}}}"#,
        )
        .unwrap();
        std::fs::write(
            home.path().join(".claude.json"),
            r#"{"mcpServers":{"playwright":{"command":"other"},"docs":{"type":"http","url":"https://docs.example.com/mcp"}}}"#,
        )
        .unwrap();
        std::fs::write(
            codex.path().join("config.toml"),
            "[mcp_servers.tickets]\ncommand = \"tickets-mcp\"\nargs = [\"--ro\"]\n",
        )
        .unwrap();
        let sources = Sources {
            repo: repo.path(),
            home: home.path(),
            codex_home: codex.path(),
        };

        let names = ["playwright", "docs", "tickets"].map(String::from);
        let servers = resolve(&names, &sources).unwrap();
        assert_eq!(
            servers[0].def,
            McpDef::Stdio {
                command: "npx".into(),
                args: vec!["@playwright/mcp".into(), "--headless".into()],
                env: BTreeMap::new()
            }
        );
        assert!(
            matches!(&servers[1].def, McpDef::Http { url, .. } if url == "https://docs.example.com/mcp")
        );
        assert!(
            matches!(&servers[2].def, McpDef::Stdio { command, .. } if command == "tickets-mcp")
        );

        let err = resolve(&["missing".to_string()], &sources).unwrap_err();
        assert!(err.contains("\"missing\""), "{err}");
    }

    #[test]
    fn each_cli_receives_the_servers_in_its_own_shape() {
        let servers = vec![
            McpServer {
                name: "playwright".into(),
                def: McpDef::Stdio {
                    command: "npx".into(),
                    args: vec!["@playwright/mcp".into()],
                    env: BTreeMap::new(),
                },
            },
            McpServer {
                name: "docs".into(),
                def: McpDef::Http {
                    url: "https://docs.example.com/mcp".into(),
                    headers: BTreeMap::new(),
                },
            },
        ];
        let claude = claude_config(&servers);
        assert_eq!(claude["mcpServers"]["playwright"]["command"], "npx");
        assert_eq!(claude["mcpServers"]["docs"]["type"], "http");
        let opencode = opencode_config(&servers, &["docs".to_owned(), "linear".to_owned()], false);
        assert_eq!(
            opencode["playwright"]["command"],
            json!(["npx", "@playwright/mcp"])
        );
        assert_eq!(opencode["docs"]["type"], "remote");
        assert_eq!(opencode["linear"], json!({"enabled": false}));
        let opencode = opencode_config(&servers, &["linear".to_owned()], true);
        assert_eq!(opencode["servers"]["docs"]["type"], "remote");
        assert_eq!(
            opencode["servers"]["linear"],
            json!({"type": "local", "command": ["true"], "disabled": true})
        );
    }

    #[test]
    fn the_servers_opencode_defines_are_read_from_its_configuration_with_comments() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("opencode.jsonc"),
            "{\n  // the team's tracker\n  \"mcp\": {\"linear\": {\"type\": \"remote\", \"url\": \"https://a//b\"}}\n}",
        )
        .unwrap();
        assert!(opencode_defined(repo.path()).contains(&"linear".to_owned()));
        std::fs::write(
            repo.path().join("opencode.json"),
            r#"{"mcp": {"servers": {"github": {"type": "remote", "url": "https://x"}}}}"#,
        )
        .unwrap();
        let defined = opencode_defined(repo.path());
        assert!(defined.contains(&"github".to_owned()) && !defined.contains(&"servers".to_owned()));
    }
}
