#!/bin/sh
# PostToolUse hook for Claude Code (Edit, Write, MultiEdit) and Codex (apply_patch). When an edit
# adds a lint suppression, inline or in a lint configuration file, it points the agent to
# AGENTS.md's rule on suppressions at that moment. It never blocks the edit.
set -eu

input=$(cat)

# Each added line as "<file>\t<line>". Claude Code gives the new text (an Edit's lines already in
# its old text are left out); Codex gives a patch, whose added lines start with "+".
added=$(printf '%s' "$input" | jq -r '
  if .tool_name == "apply_patch" then empty
  else
    .tool_input as $t
    | ([$t] + ($t.edits // []))[]
    | ((.new_string // .content // "") | split("\n")) - ((.old_string // "") | split("\n"))
    | .[]
    | "\($t.file_path)\t\(.)"
  end')
if [ -z "$added" ]; then
  added=$(printf '%s' "$input" | jq -r 'select(.tool_name == "apply_patch") | .tool_input.command // empty' |
    awk '/^\*\*\* (Add|Update) File: /{ f = $0; sub(/^\*\*\* (Add|Update) File: /, "", f); next }
         /^\+/ { print f "\t" substr($0, 2) }')
fi

found=$(printf '%s\n' "$added" | awk -F '\t' '
  $2 ~ /#!?\[(expect|allow)\(|biome-ignore|@ts-expect-error|@ts-ignore|eslint-disable/ { print $2; next }
  $1 ~ /(^|\/)(Cargo\.toml|clippy\.toml|biome\.jsonc?)$/ && $2 ~ /[=:][[:space:]]*"(allow|off)"/ { print $2 }
' | sed 's/^[[:space:]]*//' | head -n 3)
[ -n "$found" ] || exit 0

jq -n --arg lines "$found" '{
  hookSpecificOutput: {
    hookEventName: "PostToolUse",
    additionalContext: ("This edit adds a lint suppression:\n" + $lines + "\nApply the rule on lint suppressions in AGENTS.md (Implementation Quality) before keeping it.")
  }
}'
