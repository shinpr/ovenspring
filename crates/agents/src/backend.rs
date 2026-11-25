//! Turning one launch into the command line of its agent CLI, and reading what the CLI
//! printed back into a session id, token usage, and a failure classification.

use std::path::{Path, PathBuf};

use ovenspring_core::config::{Cli, Permissions};
use serde_json::{Value, json};

use crate::guard::GuardRules;
use crate::prices::Tokens;

/// Everything the adapters need to start one agent.
#[derive(Debug, Clone)]
pub struct Launch {
    pub item_name: String,
    /// 1 for the first launch of the item, then one more for each relaunch.
    pub attempt: u32,
    pub cli: Cli,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permissions: Permissions,
    /// The issue worktree (or the repository for steps that serve no issue).
    pub cwd: PathBuf,
    /// A directory owned by this launch, for the files the adapter writes.
    pub launch_dir: PathBuf,
    /// The `ovenspring` executable the hook runs.
    pub ovenspring: PathBuf,
    /// The MCP servers the owner allows this role, resolved by name.
    pub mcp: Vec<crate::mcp::McpServer>,
    /// Directories outside the worktree the agent works in: the state directory, which it reads
    /// (its item names records and earlier outputs there), and the repository's git directory
    /// (where a worktree's commits land). Claude Code gets them as added directories.
    pub reachable: Vec<PathBuf>,
    /// Of those, where Codex's sandbox lets the agent write: the git directory and the parts of
    /// the state directory agents write (items, outputs, apps, sprint facts), never the owner's
    /// decisions and approvals the merge gate trusts.
    pub writable: Vec<PathBuf>,
    pub guard: GuardRules,
    /// OpenCode 2 or later, which takes its permissions in its own form (`permissions`).
    pub opencode_v2: bool,
    /// The repository's own skills the role selects; with the factory's skills, the only ones
    /// the launch keeps.
    pub selected_skills: Vec<String>,
    /// The repository's other skills in the working directory, switched off for the launch.
    pub unselected_skills: Vec<crate::assets::RepoSkill>,
    pub prompt: String,
    /// The session this launch continues, its CLI's own id: the launch resumes it with `prompt`
    /// as the next turn, instead of starting a new session.
    pub resume: Option<String>,
}

/// A command ready to spawn, plus the files it expects to exist first.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    /// Written to the child's stdin and then closed; `None` passes the prompt as an argument.
    pub stdin: Option<String>,
    pub files: Vec<(PathBuf, String)>,
}

/// Where the user's own Codex configuration lives, so a launch can switch its parts off.
#[derive(Debug, Clone, Default)]
pub struct CodexUserConfig {
    /// Skill files found under the user's skill directories.
    pub skill_files: Vec<PathBuf>,
    /// Names of MCP servers in the user's `config.toml`.
    pub mcp_servers: Vec<String>,
    /// Why the user's `config.toml` is there but cannot be read: a Codex launch could not switch
    /// off the MCP servers it defines, so none starts.
    pub unreadable: Option<String>,
}

pub fn command(launch: &Launch, codex_user: &CodexUserConfig) -> CommandSpec {
    let mut env = vec![("OVENSPRING_LAUNCH".to_string(), launch.item_name.clone())];
    let rules_file = launch.launch_dir.join("guard.json");
    let rules_json = serde_json::to_string(&launch.guard).unwrap_or_default();
    let hook = format!(
        "{} hook --rules {}",
        shell_quote(&launch.ovenspring.display().to_string()),
        shell_quote(&rules_file.display().to_string())
    );
    let mut files = vec![(rules_file.clone(), rules_json)];

    let (program, args, stdin) = match launch.cli {
        Cli::Claude => {
            let settings_file = launch.launch_dir.join("claude-settings.json");
            // `plugin-authoring` ships with Claude Code outside its bundled skills, so
            // `disableBundledSkills` leaves it on.
            let off: serde_json::Map<String, serde_json::Value> = launch
                .unselected_skills
                .iter()
                .flat_map(|s| &s.names)
                .map(String::as_str)
                .chain(["plugin-authoring"])
                .map(|n| (n.to_owned(), json!("off")))
                .collect();
            let settings = json!({
                // Every tool, not only the shell: the hook also denies any call that names a protected path.
                "hooks": { "PreToolUse": [ { "matcher": "*", "hooks": [ { "type": "command", "command": hook } ] } ] },
                "skillOverrides": off,
                // Claude Code's bundled skills (`code-review`, `security-review`, ...) stay out of
                // a factory launch, which runs its own steps.
                "disableBundledSkills": true,
            });
            files.push((settings_file.clone(), settings.to_string()));
            let mcp_file = launch.launch_dir.join("mcp.json");
            files.push((
                mcp_file.clone(),
                crate::mcp::claude_config(&launch.mcp).to_string(),
            ));
            (
                "claude".to_string(),
                claude_args(launch, &settings_file, &mcp_file),
                Some(launch.prompt.clone()),
            )
        }
        Cli::Codex => (
            "codex".to_string(),
            codex_args(launch, codex_user, &hook),
            Some(launch.prompt.clone()),
        ),
        Cli::Opencode => {
            env.extend(opencode_env(launch));
            ("opencode".to_string(), opencode_args(launch), None)
        }
    };
    CommandSpec {
        program,
        args,
        env,
        cwd: launch.cwd.clone(),
        stdin,
        files,
    }
}

fn claude_args(launch: &Launch, settings_file: &Path, mcp_file: &Path) -> Vec<String> {
    let mut args = vec!["-p".to_string()];
    push_opt(&mut args, "--model", launch.model.as_deref());
    push_opt(&mut args, "--effort", launch.effort.as_deref());
    match launch.permissions {
        Permissions::Auto => args.extend(["--permission-mode".into(), "auto".into()]),
        Permissions::Bypass => args.push("--dangerously-skip-permissions".into()),
    }
    for dir in &launch.reachable {
        args.extend(["--add-dir".into(), dir.display().to_string()]);
    }
    args.extend([
        "--settings".into(),
        settings_file.display().to_string(),
        "--setting-sources".into(),
        "project,local".into(),
        "--strict-mcp-config".into(),
        "--mcp-config".into(),
        mcp_file.display().to_string(),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
    ]);
    match &launch.resume {
        Some(session) => args.extend(["--resume".into(), session.clone()]),
        None => args.extend([
            "--session-id".into(),
            session_uuid(&launch.item_name, launch.attempt),
        ]),
    }
    args
}

/// `hook` is the shell command Codex runs as the launch's `PreToolUse` hook.
fn codex_args(launch: &Launch, codex_user: &CodexUserConfig, hook: &str) -> Vec<String> {
    let mut args = vec!["exec".to_string()];
    if launch.resume.is_some() {
        args.push("resume".into());
    }
    push_opt(&mut args, "-m", launch.model.as_deref());
    if let Some(e) = &launch.effort {
        args.extend([
            "-c".into(),
            format!("model_reasoning_effort={}", toml_str(e)),
        ]);
    }
    match launch.permissions {
        Permissions::Auto => args.extend([
            if launch.resume.is_some() { "-c" } else { "-s" }.into(),
            if launch.resume.is_some() {
                "sandbox_mode=\"workspace-write\""
            } else {
                "workspace-write"
            }
            .into(),
            "-c".into(),
            "approval_policy=\"on-request\"".into(),
            "-c".into(),
            "approvals_reviewer=\"auto_review\"".into(),
            "-c".into(),
            "sandbox_workspace_write.network_access=true".into(),
            "-c".into(),
            format!(
                "sandbox_workspace_write.writable_roots=[{}]",
                launch
                    .writable
                    .iter()
                    .map(|p| toml_str(&p.display().to_string()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ]),
        Permissions::Bypass => args.push("--dangerously-bypass-approvals-and-sandbox".into()),
    }
    args.extend(codex_isolation(codex_user, launch));
    // The hook is defined for this launch only, so it is never in Codex's persisted trust
    // list; without this flag Codex skips it as "needs review".
    args.push("--dangerously-bypass-hook-trust".into());
    args.extend([
        "-c".into(),
        format!(
            "hooks.PreToolUse=[{{matcher={},hooks=[{{type=\"command\",command={}}}]}}]",
            toml_str("*"),
            toml_str(hook)
        ),
        "--json".into(),
    ]);
    match &launch.resume {
        Some(session) => args.push(session.clone()),
        None => args.extend(["-C".into(), launch.cwd.display().to_string()]),
    }
    args.push("-".into());
    args
}

/// `-c` overrides that define one MCP server for Codex. An environment value written as
/// `${NAME}` is forwarded by name (`env_vars`), keeping the value off the command line.
fn codex_mcp_overrides(server: &crate::mcp::McpServer) -> Vec<String> {
    use crate::mcp::McpDef;
    let key = format!("mcp_servers.{}", toml_key(&server.name));
    match &server.def {
        McpDef::Stdio { command, args, env } => {
            let forwarded: Vec<String> = env
                .iter()
                .filter(|(k, v)| **v == format!("${{{k}}}"))
                .map(|(k, _)| toml_str(k))
                .collect();
            let literal: Vec<String> = env
                .iter()
                .filter(|(k, v)| **v != format!("${{{k}}}"))
                .map(|(k, v)| format!("{}={}", toml_key(k), toml_str(v)))
                .collect();
            vec![
                format!("{key}.command={}", toml_str(command)),
                format!(
                    "{key}.args=[{}]",
                    args.iter()
                        .map(|a| toml_str(a))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                format!("{key}.env={{{}}}", literal.join(",")),
                format!("{key}.env_vars=[{}]", forwarded.join(",")),
            ]
        }
        McpDef::Http { url, headers } => {
            // A header written as `${NAME}`, or `Bearer ${NAME}` for authorization, takes the
            // variable's value by name; any other is sent as written.
            let mut overrides = vec![format!("{key}.url={}", toml_str(url))];
            let mut from_env = Vec::new();
            let mut literal = Vec::new();
            for (name, value) in headers {
                let bearer = value
                    .strip_prefix("Bearer ${")
                    .and_then(|v| v.strip_suffix('}'))
                    .filter(|_| name.eq_ignore_ascii_case("authorization"));
                let whole = value.strip_prefix("${").and_then(|v| v.strip_suffix('}'));
                match (bearer, whole) {
                    (Some(var), _) => {
                        overrides.push(format!("{key}.bearer_token_env_var={}", toml_str(var)));
                    }
                    (None, Some(var)) => {
                        from_env.push(format!("{}={}", toml_key(name), toml_str(var)));
                    }
                    (None, None) => {
                        literal.push(format!("{}={}", toml_key(name), toml_str(value)));
                    }
                }
            }
            if !literal.is_empty() {
                overrides.push(format!("{key}.http_headers={{{}}}", literal.join(",")));
            }
            if !from_env.is_empty() {
                overrides.push(format!("{key}.env_http_headers={{{}}}", from_env.join(",")));
            }
            overrides
        }
    }
}

/// Whether a launch on `cli` is OpenCode 2 or later, as `opencode --version` on `path` reports.
pub fn opencode_v2(cli: Cli, path: &str) -> bool {
    cli == Cli::Opencode && opencode_major(path).is_some_and(|major| major >= 2)
}

fn opencode_major(path: &str) -> Option<u32> {
    let out = std::process::Command::new("opencode")
        .arg("--version")
        .env("PATH", path)
        .output()
        .ok()?;
    major_of(&String::from_utf8_lossy(&out.stdout))
}

/// The major version in what `opencode --version` prints: `1.18.20` (OpenCode 1) or
/// `opencode v2.3.2` (OpenCode 2).
fn major_of(text: &str) -> Option<u32> {
    text.split_whitespace()
        .last()?
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// OpenCode has no hook; its permissions and MCP servers come from the environment, in the form
/// its version reads. They stop the guard's commands and the owner's denied ones, every skill but
/// the factory's and the selected ones, writes outside the worktree, and questions; the last
/// matching rule wins.
fn opencode_env(launch: &Launch) -> [(String, String); 2] {
    // OpenCode has no hook, so the paths the hook keeps agents from are denied as text in a
    // command, as a shell writes them (a space escaped too).
    let protected = launch.guard.protected.iter().flat_map(|p| {
        crate::guard::spellings_of(p).into_iter().flat_map(|s| {
            let escaped = s.replace(' ', "\\ ");
            [format!("*{s}*"), format!("*{escaped}*")]
        })
    });
    let mut denied: Vec<String> = crate::guard::ALWAYS_DENIED
        .iter()
        .flat_map(|d| d.patterns.iter().map(|p| (*p).to_owned()))
        .chain(launch.guard.deny.iter().cloned())
        .chain(protected)
        .collect();
    denied.dedup();
    let mut skills = vec!["*".to_owned(), format!("{}*", crate::assets::SKILL_PREFIX)];
    skills.extend(launch.selected_skills.iter().cloned());
    let reachable: Vec<String> = launch
        .reachable
        .iter()
        .flat_map(|d| [d.display().to_string(), format!("{}/*", d.display())])
        .collect();
    let permission = if launch.opencode_v2 {
        let rule = |action: &str, resource: &str, effect: &str| json!({"action": action, "resource": resource, "effect": effect});
        let mut rules = vec![rule("edit", "*", "allow"), rule("shell", "*", "allow")];
        rules.extend(denied.iter().map(|p| rule("shell", p, "deny")));
        rules.push(rule("external_directory", "*", "deny"));
        rules.extend(
            reachable
                .iter()
                .map(|d| rule("external_directory", d, "allow")),
        );
        rules.push(rule("question", "*", "deny"));
        rules.extend(
            skills
                .iter()
                .enumerate()
                .map(|(i, s)| rule("skill", s, if i == 0 { "deny" } else { "allow" })),
        );
        ("permissions", json!(rules))
    } else {
        let mut bash = serde_json::Map::new();
        bash.insert("*".into(), json!("allow"));
        for p in &denied {
            bash.insert(p.clone(), json!("deny"));
        }
        let mut external = serde_json::Map::new();
        external.insert("*".into(), json!("deny"));
        for d in &reachable {
            external.insert(d.clone(), json!("allow"));
        }
        let skill: serde_json::Map<String, serde_json::Value> = skills
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), json!(if i == 0 { "deny" } else { "allow" })))
            .collect();
        (
            "permission",
            json!({ "edit": "allow", "bash": bash, "external_directory": external, "question": "deny", "skill": skill }),
        )
    };
    let mut config = serde_json::Map::new();
    config.insert(permission.0.into(), permission.1);
    config.insert(
        "mcp".into(),
        crate::mcp::opencode_config(
            &launch.mcp,
            &crate::mcp::opencode_defined(&launch.cwd),
            launch.opencode_v2,
        ),
    );
    [
        (
            "OPENCODE_CONFIG_CONTENT".into(),
            serde_json::Value::Object(config).to_string(),
        ),
        ("PWD".into(), launch.cwd.display().to_string()),
    ]
}

fn opencode_args(launch: &Launch) -> Vec<String> {
    let mut args = vec!["run".to_string(), "--format".into(), "json".into()];
    // Version 2 runs every client through one shared background server, whose configuration
    // applies, not the launch's: a private server is the only way the launch's permissions, MCP
    // servers and skills hold.
    if launch.opencode_v2 {
        args.push("--standalone".into());
    }
    if let Some(model) = &launch.model {
        let model = match &launch.effort {
            Some(e) => format!("{model}#{e}"),
            None => model.clone(),
        };
        args.extend(["--model".into(), model]);
    }
    if let Some(session) = &launch.resume {
        args.extend(["--session".into(), session.clone()]);
    }
    args.push(launch.prompt.clone());
    args
}

/// Codex `-c` overrides that keep the user's own skills, MCP servers and ChatGPT connectors out of
/// a factory launch, and raise the tool output budgets so long reads are not cut silently.
fn codex_isolation(user: &CodexUserConfig, launch: &Launch) -> Vec<String> {
    const OUTPUT_CEILING: u32 = 40_000;
    let mut args = Vec::new();
    let mut c = |v: String| {
        args.push("-c".to_string());
        args.push(v);
    };
    let skills: Vec<String> = user
        .skill_files
        .iter()
        .chain(launch.unselected_skills.iter().map(|s| &s.file))
        .map(|p| {
            format!(
                "{{path={},enabled=false}}",
                toml_str(&p.display().to_string())
            )
        })
        .collect();
    c(format!("skills.config=[{}]", skills.join(",")));
    // ChatGPT connectors act in the user's name, the external browser is the user's own signed-in
    // Chrome, image generation spends money, and plugins (which carry computer use) and browser use
    // act on the user's desktop: all off. Plugin-provided MCP servers cannot be switched off one by
    // one, hence the feature flags; a browser for web work comes from the MCP allow-list.
    for feature in [
        "apps",
        "browser_use_external",
        "image_generation",
        "plugins",
        "computer_use",
        "browser_use",
    ] {
        c(format!("features.{feature}=false"));
    }
    // The user's own servers stay off unless allowed; an allowed server the user's Codex does not
    // define is added from its resolved definition.
    let allowed = |name: &str| launch.mcp.iter().any(|m| m.name == name);
    for server in user.mcp_servers.iter().filter(|s| !allowed(s)) {
        c(format!("mcp_servers.{}.enabled=false", toml_key(server)));
    }
    for server in launch
        .mcp
        .iter()
        .filter(|m| !user.mcp_servers.contains(&m.name))
    {
        for value in codex_mcp_overrides(server) {
            c(value);
        }
    }
    // The user's turn-end notifier runs their desktop programs; a factory launch has no use for it.
    c("notify=[]".into());
    c(format!("tool_output_token_limit={OUTPUT_CEILING}"));
    c(format!(
        "developer_instructions={}",
        toml_str(&format!(
            "Start every `exec` call with the first line `// @exec: {{\"max_output_tokens\": {OUTPUT_CEILING}}}` and give every \
             `exec_command` `max_output_tokens: {OUTPUT_CEILING}`. Output past the budget loses its middle, marked \
             `…N tokens truncated…`; base a judgment only on text you received, reading again any cut part it depends on."
        ))
    ));
    args
}

/// Finds the user's Codex skills and MCP servers under `codex_home` and `home`.
pub fn read_codex_user_config(codex_home: &Path, home: &Path) -> CodexUserConfig {
    let mut skill_files = Vec::new();
    for base in [codex_home.join("skills"), home.join(".agents/skills")] {
        collect_skill_files(&base, &mut skill_files);
    }
    skill_files.sort();
    let path = codex_home.join("config.toml");
    // Without it no MCP server of the owner's is switched off for Codex launches, so a file that
    // is there but cannot be read stops them, with why.
    let mut unreadable = None;
    let config = match std::fs::read_to_string(&path) {
        Ok(text) => text
            .parse::<toml::Table>()
            .map_err(|e| unreadable = Some(format!("{}: {e}", path.display())))
            .ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            unreadable = Some(format!("{}: {e}", path.display()));
            None
        }
    };
    let keys = |section: &str| -> Vec<String> {
        config
            .as_ref()
            .and_then(|t| t.get(section))
            .and_then(toml::Value::as_table)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    };
    CodexUserConfig {
        skill_files,
        mcp_servers: keys("mcp_servers"),
        unreadable,
    }
}

fn collect_skill_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skill_files(&path, out);
        } else if path.file_name().is_some_and(|n| n == "SKILL.md") {
            out.push(path);
        }
    }
}

fn push_opt(args: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(v) = value {
        args.push(flag.to_string());
        args.push(v.to_string());
    }
}

fn toml_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("{s:?}"))
}

fn toml_key(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        s.to_string()
    } else {
        toml_str(s)
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A UUID-shaped session id for one launch of an item: a relaunch gets a new one, since Claude
/// Code refuses a session id that is already in use, and two daemons never share one.
pub fn session_uuid(item_name: &str, attempt: u32) -> String {
    use sha2::{Digest, Sha256};
    let seed = format!("{item_name}:{}:{attempt}", std::process::id());
    let h = Sha256::digest(seed.as_bytes());
    let mut b = [0u8; 16];
    for (dst, src) in b.iter_mut().zip(h.iter()) {
        *dst = *src;
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex = ovenspring_store::hex(b.iter());
    format!(
        "{}-{}-{}-{}-{}",
        hex.get(0..8).unwrap_or_default(),
        hex.get(8..12).unwrap_or_default(),
        hex.get(12..16).unwrap_or_default(),
        hex.get(16..20).unwrap_or_default(),
        hex.get(20..32).unwrap_or_default()
    )
}

/// How a finished launch failed, from its exit code and output.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// Worth relaunching after a wait: the provider was overloaded or failed on its side.
    Transient(String),
    /// Would recur on every launch until a person acts: usage limit, signed out, unknown errors.
    Fatal(String),
}

/// What the CLI's JSON event stream says about the launch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub session: Option<String>,
    /// The model the CLI says it ran, when its stream names it.
    pub model: Option<String>,
    pub usage: Tokens,
    pub cost_usd: Option<f64>,
    pub error: Option<String>,
    /// The agent's last message: what it says it did.
    pub message: Option<String>,
    /// Every message the agent wrote, in order.
    pub messages: Vec<String>,
}

impl Outcome {
    fn said(&mut self, text: &str) {
        if !text.trim().is_empty() {
            self.messages.push(text.to_owned());
        }
        self.message = Some(text.to_owned());
    }
}

pub fn read_stream(cli: Cli, output: &str) -> Outcome {
    let mut o = Outcome::default();
    for line in output.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match cli {
            Cli::Claude => read_claude_event(&v, &mut o),
            Cli::Codex => read_codex_event(&v, &mut o),
            Cli::Opencode => read_opencode_event(&v, &mut o),
        }
    }
    if cli == Cli::Codex {
        // Codex counts cached input inside its input; the others count it apart.
        o.usage.input = o.usage.input.saturating_sub(o.usage.cache_read);
    }
    o
}

fn u(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn read_claude_event(v: &Value, o: &mut Outcome) {
    if let Some(s) = v.get("session_id").and_then(Value::as_str) {
        o.session = Some(s.to_string());
    }
    if v.get("type").and_then(Value::as_str) == Some("system")
        && let Some(model) = v.get("model").and_then(Value::as_str)
    {
        o.model = Some(model.to_owned());
    }
    if v.get("type").and_then(Value::as_str) == Some("assistant") {
        let texts = v
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|c| c.get("text").and_then(Value::as_str));
        for text in texts {
            o.said(text);
        }
    }
    if v.get("type").and_then(Value::as_str) == Some("result") {
        if let Some(usage) = v.get("usage") {
            o.usage = claude_tokens(usage);
        }
        o.cost_usd = v.get("total_cost_usd").and_then(Value::as_f64);
        // The result repeats the last message the agent wrote, unless it wrote none.
        if let Some(result) = v.get("result").and_then(Value::as_str)
            && o.message.as_deref() != Some(result)
        {
            o.said(result);
        }
        if v.get("is_error").and_then(Value::as_bool) == Some(true) {
            o.error = Some(error_text(v));
        }
    }
}

fn claude_tokens(usage: &Value) -> Tokens {
    Tokens {
        input: u(usage, "input_tokens"),
        output: u(usage, "output_tokens"),
        cache_read: u(usage, "cache_read_input_tokens"),
        cache_write: u(usage, "cache_creation_input_tokens"),
    }
}

fn read_codex_event(v: &Value, o: &mut Outcome) {
    let kind = v.get("type").and_then(Value::as_str).unwrap_or_default();
    if let Some(s) = v
        .get("thread_id")
        .or_else(|| v.get("session_id"))
        .and_then(Value::as_str)
    {
        o.session = Some(s.to_string());
    }
    if let Some(usage) = v.get("usage") {
        o.usage.input += u(usage, "input_tokens");
        o.usage.output += u(usage, "output_tokens");
        o.usage.cache_read += u(usage, "cached_input_tokens");
    }
    if kind == "error" || kind == "turn.failed" {
        o.error = Some(error_text(v));
    }
    if kind == "item.completed"
        && v.pointer("/item/type").and_then(Value::as_str) == Some("agent_message")
        && let Some(text) = v.pointer("/item/text").and_then(Value::as_str)
    {
        o.said(text);
    }
}

fn read_opencode_event(v: &Value, o: &mut Outcome) {
    if let Some(s) = v
        .get("sessionID")
        .or_else(|| v.get("session_id"))
        .and_then(Value::as_str)
    {
        o.session = Some(s.to_string());
    }
    if v.get("type").and_then(Value::as_str) == Some("error") {
        o.error = Some(error_text(v));
    }
    if v.get("type").and_then(Value::as_str) == Some("text")
        && let Some(text) = v.pointer("/part/text").and_then(Value::as_str)
    {
        o.said(text);
    }
    // Each step reports what it used. A provider OpenCode has no prices for reports a cost of 0,
    // which is not known to be free: the launch then counts as one with no cost reported.
    if v.get("type").and_then(Value::as_str) == Some("step_finish")
        && let Some(part) = v.get("part")
    {
        let tokens = |path: &str| part.pointer(path).and_then(Value::as_u64).unwrap_or(0);
        o.usage.input += tokens("/tokens/input");
        o.usage.output += tokens("/tokens/output") + tokens("/tokens/reasoning");
        o.usage.cache_read += tokens("/tokens/cache/read");
        o.usage.cache_write += tokens("/tokens/cache/write");
        if let Some(cost) = part
            .get("cost")
            .and_then(Value::as_f64)
            .filter(|c| *c > 0.0)
        {
            o.cost_usd = Some(o.cost_usd.unwrap_or(0.0) + cost);
        }
    }
}

/// An error's message: its own text, or the text nested under it. OpenCode puts a provider's
/// message under `error.data`, beside the provider's whole response.
fn error_text(v: &Value) -> String {
    for key in ["error", "message", "result", "subtype", "data"] {
        match v.get(key) {
            Some(Value::String(s)) if !s.is_empty() => return s.clone(),
            Some(obj @ Value::Object(_)) => return error_text(obj),
            Some(_) | None => {}
        }
    }
    v.to_string()
}

/// The error kinds worth relaunching after a wait.
pub const TRANSIENT_KINDS: [&str; 1] = ["server_overloaded"];

/// An error a CLI recorded in a launch's session: the kind the CLI gave it and its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedError {
    pub kind: String,
    pub message: String,
}

/// Where the CLIs keep their sessions.
#[derive(Debug, Clone, Copy)]
pub struct SessionPlaces<'a> {
    pub home: &'a Path,
    pub codex_home: &'a Path,
}

/// The last error the CLI recorded in `session`: Codex's
/// `codex_error_info` in its rollout file, Claude Code's `error` on an API error message in its
/// project's session file. OpenCode records none the factory reads.
pub fn recorded_error(cli: Cli, session: &str, at: &SessionPlaces<'_>) -> Option<RecordedError> {
    match cli {
        Cli::Claude => claude_recorded_error(session, at.home),
        Cli::Codex => codex_recorded_error(session, at.codex_home),
        Cli::Opencode => None,
    }
}

fn json_lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// Claude Code keeps each session at `.claude/projects/<the project's path>/<session>.jsonl`.
fn claude_session(session: &str, home: &Path) -> Vec<Value> {
    let file = format!("{session}.jsonl");
    std::fs::read_dir(home.join(".claude/projects"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|project| project.path().join(&file))
        .find(|path| path.is_file())
        .map(|path| json_lines(&path))
        .unwrap_or_default()
}

fn claude_recorded_error(session: &str, home: &Path) -> Option<RecordedError> {
    let record = claude_session(session, home)
        .into_iter()
        .rfind(|r| r.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true))?;
    let message = record
        .pointer("/message/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    Some(RecordedError {
        kind: text_of(record.get("error")),
        message,
    })
}

/// Completes `outcome` from what the CLI recorded in `session` beside its stream: the model, which
/// Codex's stream does not name, and the tokens used up to wherever the launch ended, which a
/// launch stopped before its stream's final event reports nowhere else.
pub fn read_session(cli: Cli, session: &str, outcome: &mut Outcome, at: &SessionPlaces<'_>) {
    match cli {
        Cli::Codex => {
            let records: Vec<Value> = codex_rollouts(session, at.codex_home)
                .iter()
                .flat_map(|f| json_lines(f))
                .collect();
            if outcome.model.is_none() {
                outcome.model = records
                    .iter()
                    .filter(|r| r.get("type").and_then(Value::as_str) == Some("turn_context"))
                    .find_map(|r| r.pointer("/payload/model").and_then(Value::as_str))
                    .map(str::to_owned);
            }
            // Each token count holds the session's total so far, cache writes included, which the
            // stream leaves out. Its input includes what was read from or written to the cache.
            if let Some(total) = records
                .iter()
                .rev()
                .find_map(|r| r.pointer("/payload/info/total_token_usage"))
            {
                let cache_read = u(total, "cached_input_tokens");
                let cache_write = u(total, "cache_write_input_tokens");
                outcome.usage = Tokens {
                    input: u(total, "input_tokens").saturating_sub(cache_read + cache_write),
                    output: u(total, "output_tokens"),
                    cache_read,
                    cache_write,
                };
            }
        }
        Cli::Claude if outcome.usage == Tokens::default() => {
            // A message is recorded once per part it wrote, each time with its usage so far.
            let mut by_message = std::collections::BTreeMap::new();
            for record in claude_session(session, at.home) {
                if let (Some(id), Some(usage)) = (
                    record.pointer("/message/id").and_then(Value::as_str),
                    record.pointer("/message/usage"),
                ) {
                    by_message.insert(id.to_owned(), claude_tokens(usage));
                }
            }
            outcome.usage = by_message
                .values()
                .fold(Tokens::default(), |sum, t| Tokens {
                    input: sum.input + t.input,
                    output: sum.output + t.output,
                    cache_read: sum.cache_read + t.cache_read,
                    cache_write: sum.cache_write + t.cache_write,
                });
        }
        Cli::Claude | Cli::Opencode => {}
    }
}

fn codex_recorded_error(session: &str, codex_home: &Path) -> Option<RecordedError> {
    let error = codex_rollouts(session, codex_home)
        .iter()
        .flat_map(|f| json_lines(f))
        .filter_map(|r| {
            r.pointer("/payload/error")
                .filter(|e| e.is_object())
                .cloned()
        })
        .next_back()?;
    Some(RecordedError {
        kind: text_of(error.get("codex_error_info")),
        message: text_of(error.get("message")),
    })
}

/// Codex keeps each session at `sessions/<year>/<month>/<day>/rollout-<time>-<session>.jsonl`.
fn codex_rollouts(session: &str, codex_home: &Path) -> Vec<PathBuf> {
    let suffix = format!("-{session}.jsonl");
    let mut files = Vec::new();
    let mut dirs = vec![(codex_home.join("sessions"), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if depth < 3 && path.is_dir() {
                dirs.push((path, depth + 1));
            } else if depth == 3
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix))
            {
                files.push(path);
            }
        }
    }
    files
}

/// How a finished launch failed, if it did. Transient only for an error kind the CLI recorded as one worth a
/// wait; any other failure would recur on every launch, so the factory pauses.
pub fn classify(
    exit_ok: bool,
    outcome: &Outcome,
    recorded: Option<&RecordedError>,
    tail: &str,
) -> Option<Failure> {
    if exit_ok && outcome.error.is_none() {
        return None;
    }
    Some(match recorded {
        Some(r) if TRANSIENT_KINDS.contains(&r.kind.as_str()) => {
            Failure::Transient(r.message.clone())
        }
        Some(r) if !r.message.is_empty() => Failure::Fatal(r.message.clone()),
        Some(r) => Failure::Fatal(r.kind.clone()),
        None => Failure::Fatal(
            outcome
                .error
                .clone()
                .unwrap_or_else(|| tail.trim().to_owned()),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(cli: Cli, permissions: Permissions) -> Launch {
        Launch {
            item_name: "20261121-120400-implement-ACME-142".into(),
            attempt: 1,
            cli,
            model: Some("model-x".into()),
            effort: Some("high".into()),
            permissions,
            cwd: PathBuf::from("/w/ACME-142"),
            launch_dir: PathBuf::from("/s/launches/x"),
            ovenspring: PathBuf::from("/bin/ovenspring"),
            mcp: vec![],
            reachable: vec![PathBuf::from("/s"), PathBuf::from("/repo/.git")],
            writable: vec![PathBuf::from("/s/outputs"), PathBuf::from("/repo/.git")],
            guard: GuardRules::default(),
            opencode_v2: false,
            selected_skills: vec![],
            unselected_skills: vec![],
            prompt: "do the task".into(),
            resume: None,
        }
    }

    #[test]
    fn each_cli_continues_the_session_it_is_given_with_the_same_isolation() {
        let resumed = |cli| {
            let mut l = launch(cli, Permissions::Auto);
            l.resume = Some("s-7".into());
            command(&l, &CodexUserConfig::default())
        };

        let claude = resumed(Cli::Claude);
        assert!(has_pair(&claude.args, "--resume", "s-7"));
        assert!(!claude.args.contains(&"--session-id".to_string()));
        assert!(has_pair(&claude.args, "--permission-mode", "auto"));
        assert_eq!(claude.stdin.as_deref(), Some("do the task"));

        let codex = resumed(Cli::Codex);
        assert_eq!(&codex.args[..2], ["exec", "resume"]);
        assert_eq!(&codex.args[codex.args.len() - 2..], ["s-7", "-"]);
        assert!(has_pair(
            &codex.args,
            "-c",
            "sandbox_mode=\"workspace-write\""
        ));
        assert!(has_pair(
            &codex.args,
            "-c",
            "sandbox_workspace_write.writable_roots=[\"/s/outputs\",\"/repo/.git\"]"
        ));
        assert!(!codex.args.contains(&"-s".to_string()) && !codex.args.contains(&"-C".to_string()));
        assert_eq!(codex.cwd, PathBuf::from("/w/ACME-142"));

        let opencode = resumed(Cli::Opencode);
        assert!(has_pair(&opencode.args, "--session", "s-7"));
        assert_eq!(
            opencode.args.last().map(String::as_str),
            Some("do the task")
        );
    }

    #[test]
    fn each_cli_switches_off_the_repository_skills_the_role_did_not_select() {
        let with_skills = |cli| {
            let mut l = launch(cli, Permissions::Auto);
            l.selected_skills = vec!["payments-domain".into()];
            l.unselected_skills = vec![crate::assets::RepoSkill {
                file: PathBuf::from("/w/ACME-142/.claude/skills/deploy/SKILL.md"),
                names: vec!["deploy".into(), "ship-it".into()],
            }];
            command(&l, &CodexUserConfig::default())
        };

        let claude = with_skills(Cli::Claude);
        let (_, settings) = claude
            .files
            .iter()
            .find(|(p, _)| p.ends_with("claude-settings.json"))
            .unwrap();
        let settings: serde_json::Value = serde_json::from_str(settings).unwrap();
        assert_eq!(
            settings["skillOverrides"],
            json!({"deploy": "off", "ship-it": "off", "plugin-authoring": "off"})
        );
        assert_eq!(settings["disableBundledSkills"], true);

        let codex = with_skills(Cli::Codex);
        assert!(codex.args.iter().any(|a| a.starts_with("skills.config=")
            && a.contains("{path=\"/w/ACME-142/.claude/skills/deploy/SKILL.md\",enabled=false}")));

        let opencode = with_skills(Cli::Opencode);
        let (_, config) = opencode
            .env
            .iter()
            .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
            .unwrap();
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        let rules: Vec<(&String, &serde_json::Value)> = config["permission"]["skill"]
            .as_object()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(
            rules,
            [
                (&"*".to_owned(), &json!("deny")),
                (&"ovenspring-*".to_owned(), &json!("allow")),
                (&"payments-domain".to_owned(), &json!("allow")),
            ],
            "deny everything first, then allow the factory's and the selected skills"
        );
    }

    fn has_pair(args: &[String], a: &str, b: &str) -> bool {
        args.windows(2).any(|w| w[0] == a && w[1] == b)
    }

    #[test]
    fn claude_auto_runs_in_auto_mode_with_the_hook_and_no_user_settings() {
        let spec = command(
            &launch(Cli::Claude, Permissions::Auto),
            &CodexUserConfig::default(),
        );
        assert_eq!(spec.program, "claude");
        assert!(has_pair(&spec.args, "--permission-mode", "auto"));
        assert!(!spec.args.iter().any(|a| a.contains("dangerously")));
        assert!(has_pair(&spec.args, "--setting-sources", "project,local"));
        assert!(
            has_pair(&spec.args, "--add-dir", "/s")
                && has_pair(&spec.args, "--add-dir", "/repo/.git")
        );
        assert!(spec.args.contains(&"--strict-mcp-config".to_string()));
        let (_, settings) = spec
            .files
            .iter()
            .find(|(p, _)| p.ends_with("claude-settings.json"))
            .unwrap();
        let settings: serde_json::Value = serde_json::from_str(settings).unwrap();
        let hook = &settings["hooks"]["PreToolUse"][0];
        assert_eq!(hook["matcher"], "*");
        assert!(
            hook["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("hook --rules")
        );
        assert_eq!(spec.stdin.as_deref(), Some("do the task"));
        assert!(spec.env.contains(&(
            "OVENSPRING_LAUNCH".into(),
            "20261121-120400-implement-ACME-142".into()
        )));
    }

    #[test]
    fn bypass_replaces_the_permission_mode_and_keeps_the_hook() {
        let claude = command(
            &launch(Cli::Claude, Permissions::Bypass),
            &CodexUserConfig::default(),
        );
        assert!(
            claude
                .args
                .contains(&"--dangerously-skip-permissions".to_string())
        );
        assert!(!claude.args.contains(&"--permission-mode".to_string()));
        assert!(
            claude
                .files
                .iter()
                .any(|(p, _)| p.ends_with("claude-settings.json"))
        );

        let codex = command(
            &launch(Cli::Codex, Permissions::Bypass),
            &CodexUserConfig::default(),
        );
        assert!(
            codex
                .args
                .contains(&"--dangerously-bypass-approvals-and-sandbox".to_string())
        );
        assert!(!codex.args.contains(&"workspace-write".to_string()));
        assert!(
            codex
                .args
                .iter()
                .any(|a| a.starts_with("hooks.PreToolUse=[{matcher=\"*\","))
        );
    }

    #[test]
    fn a_codex_config_that_is_there_but_cannot_be_read_is_named() {
        let codex = tempfile::tempdir().unwrap();
        std::fs::write(codex.path().join("config.toml"), "[mcp_servers.linear\n").unwrap();
        let user = read_codex_user_config(codex.path(), codex.path());
        assert!(
            user.unreadable
                .as_deref()
                .is_some_and(|w| w.contains("config.toml")),
            "{user:?}"
        );
        let missing = tempfile::tempdir().unwrap();
        assert_eq!(
            read_codex_user_config(missing.path(), missing.path()).unreadable,
            None
        );
    }

    #[test]
    fn codex_auto_uses_the_sandbox_with_auto_review_and_switches_off_the_users_extras() {
        let user = CodexUserConfig {
            skill_files: vec![PathBuf::from("/h/.codex/skills/x/SKILL.md")],
            mcp_servers: vec!["linear".into()],
            unreadable: None,
        };
        let spec = command(&launch(Cli::Codex, Permissions::Auto), &user);
        assert!(has_pair(&spec.args, "-s", "workspace-write"));
        assert!(has_pair(
            &spec.args,
            "-c",
            "sandbox_workspace_write.writable_roots=[\"/s/outputs\",\"/repo/.git\"]"
        ));
        assert!(has_pair(
            &spec.args,
            "-c",
            "approvals_reviewer=\"auto_review\""
        ));
        assert!(has_pair(
            &spec.args,
            "-c",
            "mcp_servers.linear.enabled=false"
        ));
        assert!(
            spec.args
                .iter()
                .any(|a| a.contains("/h/.codex/skills/x/SKILL.md") && a.contains("enabled=false"))
        );
        assert!(
            spec.args
                .contains(&"--dangerously-bypass-hook-trust".to_string())
        );
        // The owner's desktop tools stay off in a factory launch.
        for feature in [
            "apps",
            "plugins",
            "computer_use",
            "browser_use",
            "browser_use_external",
            "image_generation",
        ] {
            assert!(
                has_pair(&spec.args, "-c", &format!("features.{feature}=false")),
                "{feature}"
            );
        }
        assert!(has_pair(&spec.args, "-c", "notify=[]"));
        assert!(has_pair(
            &spec.args,
            "-c",
            "model_reasoning_effort=\"high\""
        ));
        assert_eq!(spec.args.last().map(String::as_str), Some("-"));
    }

    #[test]
    fn only_allowed_mcp_servers_reach_each_cli() {
        use crate::mcp::{McpDef, McpServer};
        let user = CodexUserConfig {
            skill_files: vec![],
            mcp_servers: vec!["linear".into(), "docs".into()],
            unreadable: None,
        };
        let mut l = launch(Cli::Codex, Permissions::Auto);
        l.mcp = vec![
            McpServer {
                name: "docs".into(),
                def: McpDef::Http {
                    url: "https://docs.example.com/mcp".into(),
                    headers: std::collections::BTreeMap::default(),
                },
            },
            McpServer {
                name: "playwright".into(),
                def: McpDef::Stdio {
                    command: "npx".into(),
                    args: vec!["@playwright/mcp".into()],
                    env: [("BROWSER_TOKEN".to_string(), "${BROWSER_TOKEN}".to_string())]
                        .into_iter()
                        .collect(),
                },
            },
        ];
        let codex = command(&l, &user);
        assert!(has_pair(
            &codex.args,
            "-c",
            "mcp_servers.linear.enabled=false"
        ));
        assert!(
            !codex
                .args
                .iter()
                .any(|a| a.starts_with("mcp_servers.docs.")),
            "the user's own allowed server stays as defined"
        );
        assert!(has_pair(
            &codex.args,
            "-c",
            "mcp_servers.playwright.command=\"npx\""
        ));
        assert!(has_pair(
            &codex.args,
            "-c",
            "mcp_servers.playwright.env_vars=[\"BROWSER_TOKEN\"]"
        ));

        l.cli = Cli::Claude;
        let claude = command(&l, &user);
        let (path, json) = claude
            .files
            .iter()
            .find(|(p, _)| p.ends_with("mcp.json"))
            .unwrap();
        assert!(has_pair(
            &claude.args,
            "--mcp-config",
            &path.display().to_string()
        ));
        assert!(json.contains("playwright") && json.contains("docs") && !json.contains("linear"));

        l.cli = Cli::Opencode;
        let opencode = command(&l, &user);
        let config = &opencode
            .env
            .iter()
            .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
            .unwrap()
            .1;
        assert!(config.contains("\"playwright\"") && !config.contains("linear"));
    }

    /// The configuration an OpenCode launch receives, for its version's form.
    fn opencode_config(v2: bool) -> serde_json::Value {
        let mut l = launch(Cli::Opencode, Permissions::Auto);
        l.opencode_v2 = v2;
        l.guard.deny = vec!["kubectl * --context *prod*".into()];
        l.guard.protected = vec![std::path::PathBuf::from("/data/ovenspring/isolated")];
        l.selected_skills = vec!["payments-domain".into()];
        let spec = command(&l, &CodexUserConfig::default());
        let (_, text) = spec
            .env
            .iter()
            .find(|(k, _)| k == "OPENCODE_CONFIG_CONTENT")
            .unwrap();
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn codex_takes_an_http_servers_headers_by_variable_name_or_as_written() {
        let server = crate::mcp::McpServer {
            name: "docs".into(),
            def: crate::mcp::McpDef::Http {
                url: "https://docs.example/mcp".into(),
                headers: [
                    (
                        "Authorization".to_owned(),
                        "Bearer ${DOCS_TOKEN}".to_owned(),
                    ),
                    ("X-Team".to_owned(), "acme".to_owned()),
                ]
                .into(),
            },
        };
        let overrides = codex_mcp_overrides(&server);
        assert!(
            overrides.contains(&"mcp_servers.docs.bearer_token_env_var=\"DOCS_TOKEN\"".to_owned()),
            "{overrides:?}"
        );
        assert!(
            overrides.contains(&"mcp_servers.docs.http_headers={X-Team=\"acme\"}".to_owned()),
            "{overrides:?}"
        );
    }

    #[test]
    fn opencode_1_denies_the_guards_and_the_owners_commands_in_its_form() {
        let c = opencode_config(false);
        let bash = &c["permission"]["bash"];
        for p in crate::guard::ALWAYS_DENIED
            .iter()
            .flat_map(|d| d.patterns.iter().copied())
            .chain(["kubectl * --context *prod*", "*/data/ovenspring/isolated*"])
        {
            assert_eq!(bash[p], "deny", "{p}");
        }
        assert_eq!(bash["*"], "allow");
        assert_eq!(c["permission"]["external_directory"]["*"], "deny");
        for reached in ["/s", "/s/*", "/repo/.git", "/repo/.git/*"] {
            assert_eq!(
                c["permission"]["external_directory"][reached], "allow",
                "{reached}"
            );
        }
        assert_eq!(c["permission"]["question"], "deny");
        assert_eq!(c["permission"]["skill"]["*"], "deny");
        assert_eq!(c["permission"]["skill"]["payments-domain"], "allow");
        assert!(c.get("permissions").is_none());
    }

    #[test]
    fn opencode_2_runs_on_a_private_server_so_the_launchs_configuration_holds() {
        let mut l = launch(Cli::Opencode, Permissions::Auto);
        l.opencode_v2 = true;
        assert!(opencode_args(&l).contains(&"--standalone".to_owned()));
        l.opencode_v2 = false;
        assert!(!opencode_args(&l).contains(&"--standalone".to_owned()));
    }

    #[test]
    fn opencode_2_gets_the_same_rules_in_its_own_form() {
        let c = opencode_config(true);
        assert!(c.get("permission").is_none());
        let rules: Vec<(String, String, String)> = c["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let f = |k: &str| r[k].as_str().unwrap().to_owned();
                (f("action"), f("resource"), f("effect"))
            })
            .collect();
        let has = |a: &str, r: &str, e: &str| {
            rules
                .iter()
                .any(|x| (x.0.as_str(), x.1.as_str(), x.2.as_str()) == (a, r, e))
        };
        for p in crate::guard::ALWAYS_DENIED
            .iter()
            .flat_map(|d| d.patterns.iter().copied())
            .chain(["kubectl * --context *prod*", "*/data/ovenspring/isolated*"])
        {
            assert!(has("shell", p, "deny"), "{p}");
        }
        assert!(has("external_directory", "*", "deny"));
        assert!(has("external_directory", "/s/*", "allow"));
        assert!(has("external_directory", "/repo/.git/*", "allow"));
        assert!(has("question", "*", "deny"));
        assert!(has("skill", "payments-domain", "allow"));
        // The last matching rule wins: the broad allow and deny come before the exceptions.
        let first = |a: &str, r: &str| rules.iter().position(|x| x.0 == a && x.1 == r).unwrap();
        assert!(first("shell", "*") < first("shell", "git push*"));
        assert!(first("skill", "*") < first("skill", "payments-domain"));
    }

    #[test]
    fn opencode_versions_are_read_as_each_major_version_prints_them() {
        assert_eq!(major_of("1.18.20\n"), Some(1));
        assert_eq!(major_of("opencode v2.3.2\n"), Some(2));
        assert_eq!(major_of(""), None);
    }

    #[test]
    fn claude_code_names_its_model_in_the_streams_first_event() {
        let stream =
            r#"{"type":"system","subtype":"init","session_id":"s","model":"claude-opus-5-5"}"#;
        assert_eq!(
            read_stream(Cli::Claude, stream).model.as_deref(),
            Some("claude-opus-5-5")
        );
    }

    #[test]
    fn opencode_takes_its_permissions_from_the_environment_and_has_no_hook() {
        let spec = command(
            &launch(Cli::Opencode, Permissions::Auto),
            &CodexUserConfig::default(),
        );
        assert!(
            spec.env
                .iter()
                .any(|(k, v)| k == "OPENCODE_CONFIG_CONTENT" && v.contains("\"edit\":\"allow\""))
        );
        assert!(has_pair(&spec.args, "--model", "model-x#high"));
        assert!(spec.stdin.is_none());
        assert!(!spec.args.iter().any(|a| a.contains("hook")));
    }

    #[test]
    fn session_ids_are_uuid_shaped_and_new_for_each_relaunch() {
        let id = session_uuid("x", 1);
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'));
        assert_ne!(id, session_uuid("x", 2));
    }

    #[test]
    fn claude_result_events_give_usage_and_errors() {
        let out = r#"{"type":"system","session_id":"s-1"}
{"type":"result","is_error":false,"total_cost_usd":0.31,"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":7}}"#;
        let o = read_stream(Cli::Claude, out);
        assert_eq!(
            (
                o.session.as_deref(),
                o.usage.input,
                o.usage.cache_write,
                o.cost_usd
            ),
            (Some("s-1"), 10, 7, Some(0.31))
        );
        assert_eq!(classify(true, &o, None, ""), None);
    }

    #[test]
    fn opencode_steps_give_the_launch_its_usage_and_a_zero_cost_counts_as_unknown() {
        let unpriced = r#"{"type":"step_finish","part":{"cost":0,"tokens":{"total":10957,"input":10636,"output":127,"reasoning":194,"cache":{"write":0,"read":0}},"reason":"tool-calls"}}
{"type":"step_finish","part":{"cost":0,"tokens":{"total":14876,"input":4148,"output":33,"reasoning":71,"cache":{"write":0,"read":10624}},"reason":"stop"}}"#;
        let o = read_stream(Cli::Opencode, unpriced);
        assert_eq!(
            (
                o.usage.input,
                o.usage.output,
                o.usage.cache_read,
                o.cost_usd
            ),
            (14_784, 425, 10_624, None)
        );
        let priced = r#"{"type":"step_finish","part":{"cost":0.012,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"write":0,"read":0}}}}
{"type":"step_finish","part":{"cost":0.003,"tokens":{"input":1,"output":1,"reasoning":0,"cache":{"write":0,"read":0}}}}"#;
        let cost = read_stream(Cli::Opencode, priced).cost_usd.unwrap();
        assert!((cost - 0.015).abs() < 1e-9, "{cost}");
    }

    #[test]
    fn an_opencode_provider_error_reads_as_the_provider_s_message() {
        let failed = r#"{"type":"error","error":{"name":"APIError","data":{"message":"Insufficient balance or no resource package. Please recharge.","statusCode":429,"isRetryable":true,"responseBody":"{\"error\":{\"code\":\"1113\"}}","metadata":{"url":"https://api.z.ai/api/paas/v4/chat/completions"}}}}"#;
        assert_eq!(
            read_stream(Cli::Opencode, failed).error.as_deref(),
            Some("Insufficient balance or no resource package. Please recharge.")
        );
    }

    #[test]
    fn each_cli_s_last_message_is_what_the_agent_says_it_did() {
        let claude = r#"{"type":"assistant","message":{}}
{"type":"result","is_error":false,"result":"Task 1 done; pnpm test passes."}"#;
        let codex = r#"{"type":"item.completed","item":{"type":"agent_message","text":"First look."}}
{"type":"item.completed","item":{"type":"command_execution","command":"ls"}}
{"type":"item.completed","item":{"type":"agent_message","text":"Task 1 done."}}"#;
        let opencode = r#"{"type":"text","part":{"type":"text","text":"Task 1 done."}}
{"type":"step_finish"}"#;
        assert_eq!(
            read_stream(Cli::Claude, claude).message.as_deref(),
            Some("Task 1 done; pnpm test passes.")
        );
        assert_eq!(
            read_stream(Cli::Codex, codex).message.as_deref(),
            Some("Task 1 done.")
        );
        assert_eq!(
            read_stream(Cli::Opencode, opencode).message.as_deref(),
            Some("Task 1 done.")
        );
    }

    #[test]
    fn each_cli_s_messages_are_read_in_order_while_it_runs_and_after() {
        let claude = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Reading the findings."},{"type":"tool_use","name":"Read"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Two apply."}]}}
{"type":"result","is_error":false,"result":"Two apply."}"#;
        let codex = r#"{"type":"item.completed","item":{"type":"agent_message","text":"First look."}}
{"type":"item.completed","item":{"type":"command_execution","command":"ls"}}
{"type":"item.completed","item":{"type":"agent_message","text":"Task 1 done."}}"#;
        let opencode = r#"{"type":"text","part":{"type":"text","text":"Planning."}}
{"type":"text","part":{"type":"text","text":"Task 1 done."}}"#;
        assert_eq!(
            read_stream(Cli::Claude, claude).messages,
            ["Reading the findings.", "Two apply."]
        );
        assert_eq!(
            read_stream(Cli::Claude, claude.lines().next().unwrap()).messages,
            ["Reading the findings."],
            "a run still going has said what it has so far"
        );
        assert_eq!(
            read_stream(Cli::Codex, codex).messages,
            ["First look.", "Task 1 done."]
        );
        assert_eq!(
            read_stream(Cli::Opencode, opencode).messages,
            ["Planning.", "Task 1 done."]
        );
    }

    fn write(path: &Path, lines: &[Value]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text: Vec<String> = lines.iter().map(Value::to_string).collect();
        std::fs::write(path, text.join("\n")).unwrap();
    }

    #[test]
    fn the_kind_the_cli_recorded_decides_and_only_an_overload_is_retried() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let codex_home = home.join(".codex");
        let at = SessionPlaces {
            home: &home,
            codex_home: &codex_home,
        };
        write(
            &codex_home.join("sessions/2026/11/21/rollout-2026-11-21T10-00-00-t-1.jsonl"),
            &[
                json!({"type": "session_meta", "payload": {"id": "t-1"}}),
                json!({"type": "event_msg", "payload": {"error": {"codex_error_info": "server_overloaded", "message": "We're currently experiencing high demand"}}}),
            ],
        );
        let over = recorded_error(Cli::Codex, "t-1", &at).unwrap();
        assert_eq!(over.kind, "server_overloaded");
        assert_eq!(
            classify(false, &Outcome::default(), Some(&over), ""),
            Some(Failure::Transient(
                "We're currently experiencing high demand".into()
            ))
        );

        write(
            &home.join(".claude/projects/-w-ACME-1/s-2.jsonl"),
            &[
                json!({"isApiErrorMessage": true, "error": "rate_limit", "message": {"content": [{"type": "text", "text": "You've hit your usage limit"}]}}),
            ],
        );
        let limit = recorded_error(Cli::Claude, "s-2", &at).unwrap();
        assert_eq!(
            classify(false, &Outcome::default(), Some(&limit), ""),
            Some(Failure::Fatal("You've hit your usage limit".into()))
        );

        // A message naming an overload is not a recorded kind.
        let text_only = Outcome {
            error: Some("API Error: 529 overloaded_error".into()),
            ..Outcome::default()
        };
        assert_eq!(recorded_error(Cli::Claude, "missing", &at), None);
        assert_eq!(
            classify(false, &text_only, None, ""),
            Some(Failure::Fatal("API Error: 529 overloaded_error".into()))
        );
        assert_eq!(
            classify(false, &Outcome::default(), None, " segfault "),
            Some(Failure::Fatal("segfault".into()))
        );
    }

    #[test]
    fn a_stopped_codex_session_counts_its_tokens_to_its_last_count_cache_writes_apart() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        let count = |input: u64, cached: u64, written: u64, output: u64| {
            json!({"type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": {
                "input_tokens": input, "cached_input_tokens": cached,
                "cache_write_input_tokens": written, "output_tokens": output}}}})
        };
        write(
            &codex_home.join("sessions/2026/11/25/rollout-2026-11-25T10-00-00-t-3.jsonl"),
            &[
                json!({"type": "turn_context", "payload": {"model": "gpt-6.2-sol"}}),
                count(100, 40, 0, 5),
                count(900, 500, 300, 60),
            ],
        );
        let mut stopped = read_stream(Cli::Codex, r#"{"type":"thread.started","thread_id":"t-3"}"#);
        let at = SessionPlaces {
            home: dir.path(),
            codex_home: &codex_home,
        };

        read_session(Cli::Codex, "t-3", &mut stopped, &at);

        assert_eq!(stopped.model.as_deref(), Some("gpt-6.2-sol"));
        assert_eq!(
            stopped.usage,
            Tokens {
                input: 100,
                output: 60,
                cache_read: 500,
                cache_write: 300
            }
        );
    }

    #[test]
    fn a_stopped_claude_session_counts_each_message_once_at_its_final_usage() {
        let dir = tempfile::tempdir().unwrap();
        let usage = |output: u64| {
            json!({"input_tokens": 2, "output_tokens": output,
                   "cache_read_input_tokens": 100, "cache_creation_input_tokens": 10})
        };
        write(
            &dir.path().join(".claude/projects/-w-ACME-1/s-4.jsonl"),
            &[
                json!({"type": "user", "message": {"content": "do the task"}}),
                json!({"type": "assistant", "message": {"id": "m-1", "usage": usage(8)}}),
                json!({"type": "assistant", "message": {"id": "m-1", "usage": usage(300)}}),
                json!({"type": "assistant", "message": {"id": "m-2", "usage": usage(50)}}),
            ],
        );
        let mut stopped = read_stream(Cli::Claude, r#"{"type":"system","session_id":"s-4"}"#);
        let at = SessionPlaces {
            home: dir.path(),
            codex_home: &dir.path().join(".codex"),
        };

        read_session(Cli::Claude, "s-4", &mut stopped, &at);

        assert_eq!(
            stopped.usage,
            Tokens {
                input: 4,
                output: 350,
                cache_read: 200,
                cache_write: 20
            }
        );
    }
}
