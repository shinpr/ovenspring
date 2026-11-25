import { useState } from 'react'
import type { AgentsConfig, Available } from '../../api/types.gen.ts'
import { CheckGroup, RadioGroup } from '../Choices/Choices.tsx'
import { type Reasons, SettingRow } from '../SettingRow/SettingRow.tsx'
import { Warning } from '../Warning/Warning.tsx'
import styles from './AgentsSettings.module.css'

type Row = 'permissions' | 'mcp'

interface Props {
  agents: AgentsConfig
  /** The MCP servers the owner has defined, which launches may be allowed to use. */
  available: readonly Available[]
  onSave: (agents: AgentsConfig) => Promise<Reasons>
  open?: Row
}

/** What an agent could do through a server that reaches one of the factory's own services. */
const REACHES: Record<string, string> = {
  github: 'Reaches GitHub: agents could push, merge or approve through it.',
  linear: 'Reaches Linear: agents could change issues through it.',
  slack: 'Reaches Slack: agents could post as the factory through it.',
}

/** Settings that apply to every launch, whatever its role. */
export function AgentsSettings({ agents, available, onSave, open }: Props) {
  const [draft, setDraft] = useState(agents)
  const reset = (): void => setDraft(agents)
  const mcp = draft.mcp ?? []
  const save = (): Promise<Reasons> => onSave(draft)
  const known = new Set(available.map((s) => s.name))
  const gone = mcp.filter((name) => !known.has(name))
  return (
    <>
      <SettingRow
        name="Permission mode"
        what="How much the agents may do without the CLI's own permission checks."
        summary={
          agents.permissions === 'auto' ? (
            <>
              <b>Auto</b> (default): each CLI's auto mode decides which actions need review.
            </>
          ) : (
            <span className={styles['fail']}>
              <b>Bypass</b>: the CLIs' permission checks are skipped.
            </span>
          )
        }
        initiallyOpen={open === 'permissions'}
        onOpen={reset}
        onSave={save}
      >
        {() => (
          <RadioGroup
            legend="Permissions"
            legendHidden
            value={draft.permissions}
            options={[
              {
                value: 'auto',
                label: (
                  <>
                    <b>Auto</b> (default)
                  </>
                ),
                hint: "Each CLI's auto mode checks risky actions. When it refuses one, the lead finds another way or asks you.",
              },
              {
                value: 'bypass',
                label: <b>Bypass</b>,
                hint: "Skips the CLIs' permission checks for every run.",
              },
            ]}
            onChange={(permissions) => setDraft((d) => ({ ...d, permissions }))}
            after={
              draft.permissions === 'bypass' && (
                <Warning>
                  with Bypass, agents can delete files, rewrite history in their worktree and run
                  any command your denied commands do not name. Pushes, merges, approvals and your
                  denied commands are still stopped.
                </Warning>
              )
            }
          />
        )}
      </SettingRow>
      <SettingRow
        name="MCP servers agents may use"
        what="Every run gets these servers and no others."
        summary={
          (agents.mcp ?? []).length === 0 ? (
            <span className={styles['muted']}>None</span>
          ) : (
            (agents.mcp ?? []).map((name) => (
              <div key={name}>
                <b>{name}</b>{' '}
                <span className={styles['source']}>
                  {available.find((s) => s.name === name)?.source === undefined
                    ? 'no longer defined'
                    : `from ${available.find((s) => s.name === name)?.source ?? ''}`}
                </span>
              </div>
            ))
          )
        }
        initiallyOpen={open === 'mcp'}
        onOpen={reset}
        onSave={save}
      >
        {() =>
          available.length === 0 && gone.length === 0 ? (
            <p>
              No MCP servers are defined in this repository's <code>.mcp.json</code> or in your own
              Claude Code or Codex configuration.
            </p>
          ) : (
            <CheckGroup
              legend="MCP servers agents may use"
              legendHidden
              values={mcp}
              options={[
                ...available.map((s) => ({
                  value: s.name,
                  label: (
                    <>
                      <b>{s.name}</b> <span className={styles['source']}>from {s.source}</span>
                    </>
                  ),
                  ...(s.reaches.length === 0
                    ? {}
                    : {
                        hint: (
                          <span className={styles['fail']}>
                            {s.reaches.map((r) => REACHES[r] ?? `Reaches ${r}.`).join(' ')}
                          </span>
                        ),
                      }),
                })),
                ...gone.map((name) => ({
                  value: name,
                  label: <code>{name}</code>,
                  hint: (
                    <span className={styles['fail']}>
                      No longer defined anywhere. Agents fail to start until you define it again or
                      uncheck it.
                    </span>
                  ),
                })),
              ]}
              onChange={(chosen) => setDraft((d) => ({ ...d, mcp: chosen }))}
            />
          )
        }
      </SettingRow>
    </>
  )
}
