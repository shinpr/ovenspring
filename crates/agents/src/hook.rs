//! `ovenspring hook <cli> --rules <file>`: the `PreToolUse` hook of a factory launch.
//!
//! Both CLIs pass the tool call as JSON on stdin. A denied call exits with status 2 and the reason
//! on stderr, which both CLIs show the agent instead of running the tool.

use serde_json::Value;

use crate::guard::{self, GuardRules};

/// The shell command a tool call would run, if it runs one.
pub fn command_of(input: &Value) -> Option<String> {
    let tool_input = input
        .get("tool_input")
        .or_else(|| input.get("input"))
        .unwrap_or(input);
    match tool_input
        .get("command")
        .or_else(|| tool_input.get("cmd"))?
    {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let words: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            // `["bash", "-lc", "<script>"]` runs the script; anything else runs the words.
            match words.as_slice() {
                [shell, flag, script]
                    if shell.ends_with("sh") && flag.starts_with('-') && flag.ends_with('c') =>
                {
                    Some((*script).to_string())
                }
                _ => Some(words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ")),
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => None,
    }
}

/// Any tool's input (a file path, a search path or pattern, a shell command, a patch) that names a
/// protected path is denied, written as an absolute path or from the home directory. A path built
/// at run time to hide it is not caught: the hook stops honest reach and mistakes.
fn names_no_protected_path(input: &Value, rules: &GuardRules) -> Result<(), String> {
    let tool_input = input
        .get("tool_input")
        .or_else(|| input.get("input"))
        .unwrap_or(input);
    // A shell writes a space in a path as `\ `; the command is also read with those unescaped.
    let texts = [
        tool_input.to_string(),
        command_of(input).unwrap_or_default().replace("\\ ", " "),
    ];
    for path in &rules.protected {
        let full = path.display().to_string();
        if crate::guard::spellings_of(path)
            .iter()
            .any(|s| texts.iter().any(|t| t.contains(s.as_str())))
        {
            return Err(format!(
                "`{full}` holds what the factory keeps from agents (the owner's decisions and \
                 approvals, recovery procedures, credentials); agents do not read or change it. \
                 If the step needs it, set this item blocked and say why."
            ));
        }
    }
    Ok(())
}

fn quote(word: &str) -> String {
    if word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./=:@,+".contains(c))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// `Ok(())` allows the call; `Err(reason)` denies it.
pub fn decide(stdin: &str, rules: &GuardRules) -> Result<(), String> {
    let input: Value = serde_json::from_str(stdin)
        .map_err(|e| format!("the hook could not read the tool call: {e}"))?;
    names_no_protected_path(&input, rules)?;
    // Codex's `apply_patch` carries the patch in `command`: its lines are file content, not
    // commands, so only the protected paths apply to it.
    if input.get("tool_name").and_then(Value::as_str) == Some("apply_patch") {
        return Ok(());
    }
    match command_of(&input) {
        Some(cmd) => guard::check(&cmd, rules),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn rules() -> GuardRules {
        GuardRules::default()
    }

    #[test]
    fn a_patch_that_edits_a_line_naming_git_push_is_allowed() {
        let patch = "*** Begin Patch\n*** Update File: Makefile\n@@\n release:\n-\tgit push origin main\n+\tgit push origin main --tags\n*** End Patch";
        let call = json!({"tool_name": "apply_patch", "tool_input": {"command": patch}});
        assert_eq!(decide(&call.to_string(), &rules()), Ok(()));
    }

    #[test]
    fn a_claude_bash_call_that_pushes_is_denied() {
        let input = r#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"cargo test && git push"}}"#;
        assert!(
            decide(input, &rules())
                .unwrap_err()
                .contains("agents do not push")
        );
    }

    #[test]
    fn a_codex_shell_call_given_as_words_is_checked_as_its_script() {
        let input =
            r#"{"tool_name":"shell","tool_input":{"command":["bash","-lc","gh pr merge 3"]}}"#;
        assert!(decide(input, &rules()).is_err());
        let plain = r#"{"tool_name":"shell","tool_input":{"command":["git","push"]}}"#;
        assert!(decide(plain, &rules()).is_err());
    }

    #[test]
    fn calls_that_run_no_command_and_safe_commands_are_allowed() {
        assert!(
            decide(
                r#"{"tool_name":"Read","tool_input":{"file_path":"/x"}}"#,
                &rules()
            )
            .is_ok()
        );
        assert!(
            decide(
                r#"{"tool_name":"Bash","tool_input":{"command":"git status"}}"#,
                &rules()
            )
            .is_ok()
        );
    }

    fn protecting(paths: &[&str]) -> GuardRules {
        GuardRules {
            deny: vec![],
            protected: paths.iter().map(std::path::PathBuf::from).collect(),
        }
    }

    const ISOLATED: &str = "/u/Library/Application Support/ovenspring/repos/r-1/isolated";
    const CREDENTIALS: &str = "/u/.config/ovenspring/credentials/r-1.toml";

    #[test]
    fn any_tool_that_names_a_protected_path_is_denied() {
        let rules = protecting(&[ISOLATED, CREDENTIALS]);
        let calls = [
            json!({"tool_name": "Read", "tool_input": {"file_path": format!("{ISOLATED}/decisions/d-1.json")}}),
            json!({"tool_name": "Edit", "tool_input": {"file_path": format!("{ISOLATED}/recoveries/r.sh"), "old_string": "a", "new_string": "b"}}),
            json!({"tool_name": "Grep", "tool_input": {"pattern": "sha", "path": ISOLATED}}),
            json!({"tool_name": "Bash", "tool_input": {"command": format!("cat '{CREDENTIALS}'")}}),
            json!({"tool_name": "Bash", "tool_input": {"command": "cat /u/Library/Application\\ Support/ovenspring/repos/r-1/isolated/approvals/a.json"}}),
            json!({"tool_name": "apply_patch", "tool_input": {"command": format!("*** Begin Patch\n*** Update File: {ISOLATED}/recoveries/r.sh\n*** End Patch")}}),
        ];
        for call in calls {
            let reason = decide(&call.to_string(), &rules).unwrap_err();
            assert!(
                reason.contains("agents do not read or change it"),
                "{call}: {reason}"
            );
        }
    }

    #[test]
    fn a_protected_path_written_from_the_home_directory_is_denied() {
        let home = std::env::var("HOME").unwrap();
        let rules = protecting(&[&format!("{home}/state-base/r-1/isolated")]);
        for command in [
            "ls ~/state-base/r-1/isolated",
            "cat $HOME/state-base/r-1/isolated/dashboard.json",
            "cat ${HOME}/state-base/r-1/isolated/dashboard.json",
        ] {
            let call = json!({"tool_name": "Bash", "tool_input": {"command": command}});
            assert!(decide(&call.to_string(), &rules).is_err(), "{command}");
        }
    }

    #[test]
    fn tools_on_other_paths_pass_the_protected_paths_rule() {
        let rules = protecting(&[ISOLATED, CREDENTIALS]);
        let calls = [
            json!({"tool_name": "Read", "tool_input": {"file_path": "/u/Library/Application Support/ovenspring/repos/r-1/state/board/active/i.md"}}),
            json!({"tool_name": "Grep", "tool_input": {"pattern": "isolated", "path": "/w"}}),
            json!({"tool_name": "Bash", "tool_input": {"command": "git status"}}),
        ];
        for call in calls {
            assert!(decide(&call.to_string(), &rules).is_ok(), "{call}");
        }
    }

    #[test]
    fn unreadable_input_is_denied_rather_than_waved_through() {
        assert!(decide("not json", &rules()).is_err());
    }
}
