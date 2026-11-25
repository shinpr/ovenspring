import { Fragment, type ReactNode, useState } from 'react'
import type { Cli, Role, RoleConfig, RoleSection, Skills } from '../../api/types.gen.ts'
import { CLI, ROLE } from '../../lib/vocabulary.ts'
import { CheckGroup, RadioGroup } from '../Choices/Choices.tsx'
import { controlClass, Field } from '../Field/Field.tsx'
import { type Reasons, SettingRow } from '../SettingRow/SettingRow.tsx'
import { Warning } from '../Warning/Warning.tsx'
import styles from './RoleEditor.module.css'

/** What each role does, as its row says it. */
const DOES: Record<Role, string> = {
  lead: 'Proposes sprints, plans each issue, writes the documents, judges findings and talks with you in Slack.',
  executor:
    'Writes test skeletons and code, runs the quality checks, fixes CI failures and conflicts.',
  reviewer: 'Reviews documents, code, security, and what a sprint review changes.',
  analyzer: "Collects a sprint's facts for its review.",
}

/**
 * The effort levels a CLI takes from a fixed list: Claude Code's `--effort` levels, and the
 * levels every Codex model offers. OpenCode passes its provider's variant, which is free text.
 */
const EFFORTS: Partial<Record<Cli, readonly string[]>> = {
  claude: ['low', 'medium', 'high', 'xhigh', 'max'],
  codex: ['low', 'medium', 'high', 'xhigh', 'max'],
}

const ALL_CLIS: readonly Cli[] = ['claude', 'codex', 'opencode']

const MODEL_HINT: Record<Cli, ReactNode> = {
  claude: (
    <>
      As Claude Code names it, such as <code>claude-opus-5-5</code>. Leave empty for Claude Code's
      default.
    </>
  ),
  codex: (
    <>
      As Codex names it, such as <code>gpt-6-luna</code>. Leave empty for Codex's default.
    </>
  ),
  opencode: (
    <>
      As <code>provider/model</code>, the way OpenCode names it. Leave empty for OpenCode's default.
    </>
  ),
}

/** Each folder a CLI loads skills from, as code, joined by `word`. */
function folders(dirs: readonly string[], word: string): ReactNode {
  return dirs.map((d, i) => (
    <Fragment key={d}>
      {i > 0 && ` ${word} `}
      <code>{d}</code>
    </Fragment>
  ))
}

/** What each effort level does, after its name. */
const LEVEL: Record<string, string> = {
  low: 'lighter reasoning, faster',
  medium: 'balanced',
  high: 'deeper reasoning',
  xhigh: 'extra-high reasoning',
  max: 'maximum reasoning',
}

interface Draft {
  cli: Cli
  model: string
  effort: string
  skills: string[]
  append: string
}

function draftOf(config: RoleConfig): Draft {
  return {
    cli: config.cli,
    model: config.model ?? '',
    effort: config.effort ?? '',
    skills: config.skills ?? [],
    append: config.append ?? '',
  }
}

interface Props {
  role: Role
  config: RoleConfig
  skills: Skills
  onSave: (section: RoleSection) => Promise<Reasons>
  open?: boolean
}

function Summary({ config }: { config: RoleConfig }) {
  const skills = config.skills ?? []
  return (
    <dl className={styles['summary']}>
      <dt>Agent</dt>
      <dd>{CLI[config.cli]}</dd>
      <dt>Model</dt>
      <dd>
        {config.model == null ? (
          <span className={styles['muted']}>the CLI's default</span>
        ) : (
          <code>{config.model}</code>
        )}
      </dd>
      <dt>Effort</dt>
      <dd>{config.effort ?? <span className={styles['muted']}>the CLI's default</span>}</dd>
      <dt>Skills from this repository</dt>
      <dd>
        {skills.length === 0 ? (
          <span className={styles['muted']}>None (the factory's own skills always load)</span>
        ) : (
          skills.map((s, i) => (
            <span key={s}>
              {i > 0 && ' '}
              <code>{s}</code>
            </span>
          ))
        )}
      </dd>
      <dt>Additional instructions</dt>
      <dd>
        {(config.append ?? '') === '' ? (
          <span className={styles['muted']}>None</span>
        ) : (
          <span className={styles['clamp']}>{config.append}</span>
        )}
      </dd>
    </dl>
  )
}

function EffortField({ draft, set }: { draft: Draft; set: (d: Partial<Draft>) => void }) {
  const levels = EFFORTS[draft.cli]
  if (levels === undefined) {
    return (
      <Field
        label="Effort"
        hint={
          <>
            The provider's variant, such as <code>high</code>, <code>max</code> or{' '}
            <code>minimal</code>. Leave empty for the default.
          </>
        }
      >
        {(control) => (
          <input
            {...control}
            className={controlClass}
            value={draft.effort}
            onChange={(e) => set({ effort: e.target.value })}
          />
        )}
      </Field>
    )
  }
  return (
    <Field
      label="Effort"
      hint={
        draft.cli === 'claude' ? (
          <>
            The levels Claude Code's <code>--effort</code> takes.
          </>
        ) : (
          <>
            The levels every Codex model offers, as <code>model_reasoning_effort</code> takes them.
          </>
        )
      }
    >
      {(control) => (
        <select
          {...control}
          className={controlClass}
          value={draft.effort}
          onChange={(e) => set({ effort: e.target.value })}
        >
          <option value="">
            {draft.cli === 'claude' ? "Claude Code's default" : "The model's default"}
          </option>
          {levels.map((e) => (
            <option key={e} value={e}>
              {e}: {LEVEL[e]}
            </option>
          ))}
        </select>
      )}
    </Field>
  )
}

/** One role's agent, model, effort, repository skills and instructions. */
export function RoleEditor({ role, config, skills, onSave, open = false }: Props) {
  const [draft, setDraft] = useState<Draft>(() => draftOf(config))
  const [notes, setNotes] = useState<ReadonlyArray<{ key: string; text: ReactNode }>>([])
  const set = (change: Partial<Draft>): void => setDraft((d) => ({ ...d, ...change }))
  const available = skills[draft.cli]
  const dirs = folders(available.dirs, 'and')
  const skillsKey = `roles.${role}.skills`
  const choose = (cli: Cli): void => {
    const loadable = skills[cli].skills.map((s) => s.name)
    const dropped = draft.skills.filter((s) => !loadable.includes(s))
    const said: Array<{ key: string; text: ReactNode }> = []
    if (draft.model !== '' || draft.effort !== '') {
      said.push({
        key: 'model',
        text: `The model and effort set for ${CLI[draft.cli]} do not apply to ${CLI[cli]}, so they are cleared.`,
      })
    }
    for (const s of dropped) {
      said.push({
        key: s,
        text: (
          <>
            <code>{s}</code> is not in {skills[cli].dirs.join(' or ')}, where {CLI[cli]} loads
            skills, so it is unselected.
          </>
        ),
      })
    }
    setNotes(said)
    setDraft((d) => ({
      ...d,
      cli,
      model: '',
      effort: '',
      skills: d.skills.filter((s) => loadable.includes(s)),
    }))
  }
  return (
    <SettingRow
      name={ROLE[role]}
      what={DOES[role]}
      layout="block"
      applies="Applies from this role's next run."
      summary={<Summary config={config} />}
      initiallyOpen={open}
      onOpen={() => {
        setDraft(draftOf(config))
        setNotes([])
      }}
      onSave={() =>
        onSave({
          role,
          cli: draft.cli,
          model: draft.model.trim() === '' ? null : draft.model,
          effort: draft.effort.trim() === '' ? null : draft.effort,
          skills: draft.skills,
          append: draft.append,
        })
      }
      fields={[skillsKey]}
    >
      {(reasons) => (
        <>
          <RadioGroup
            legend="Agent"
            value={draft.cli}
            options={ALL_CLIS.map((cli) => ({ value: cli, label: CLI[cli] }))}
            onChange={choose}
            after={notes.map((n) => (
              <Warning key={n.key} kind="Note">
                {n.text}
              </Warning>
            ))}
          />
          <Field label="Model" hint={MODEL_HINT[draft.cli]}>
            {(control) => (
              <input
                {...control}
                className={controlClass}
                value={draft.model}
                onChange={(e) => set({ model: e.target.value })}
              />
            )}
          </Field>
          <EffortField draft={draft} set={set} />
          <CheckGroup
            legend="Skills from this repository"
            hint={<>Found in {dirs}. The ones you check load with the factory's own.</>}
            values={draft.skills}
            options={available.skills.map((s) => ({
              value: s.name,
              label: <code>{s.name}</code>,
              hint: s.description,
            }))}
            onChange={(values) => set({ skills: values })}
            after={
              <>
                {available.skills.length === 0 && (
                  <p className={styles['empty']}>No skills there yet.</p>
                )}
                {reasons[skillsKey] !== undefined && (
                  <p className={styles['error']}>{reasons[skillsKey]}</p>
                )}
              </>
            }
          />
          <Field
            label="Additional instructions"
            hint="Added as written to the end of this role's prompts. For what AGENTS.md does not say."
          >
            {(control) => (
              <textarea
                {...control}
                className={controlClass}
                rows={4}
                value={draft.append}
                onChange={(e) => set({ append: e.target.value })}
              />
            )}
          </Field>
        </>
      )}
    </SettingRow>
  )
}
