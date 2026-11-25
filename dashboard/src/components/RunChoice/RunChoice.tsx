import type { ReactNode } from 'react'
import type { AppConfig, RunRequest } from '../../api/types.gen.ts'
import { RadioGroup } from '../Choices/Choices.tsx'
import { controlClass, Field } from '../Field/Field.tsx'

export interface RunDraft {
  runs: 'commands' | 'nothing'
  setup: string
  start: string
  ready: string
}

/** The draft for what `[app]` holds, or for commands found elsewhere. */
export function runDraft(
  app: Partial<Pick<AppConfig, 'setup' | 'start' | 'ready'>>,
  nothing: boolean
): RunDraft {
  return {
    runs: nothing ? 'nothing' : 'commands',
    setup: app.setup ?? '',
    start: app.start ?? '',
    ready: app.ready ?? '',
  }
}

export function runRequest(draft: RunDraft): RunRequest {
  if (draft.runs === 'nothing') {
    return { runs: 'nothing' }
  }
  const entered = (v: string): string | null => (v.trim() === '' ? null : v.trim())
  return {
    runs: 'commands',
    setup: entered(draft.setup),
    start: draft.start.trim(),
    ready: entered(draft.ready),
  }
}

const FIELDS = [
  {
    key: 'setup',
    label: 'Prepare',
    optional: true,
    hint: 'Runs before each start of a copy, such as installing its dependencies.',
  },
  {
    key: 'start',
    label: 'Start',
    hint: (
      <>
        Starts the product on <code>$PORT</code>.
      </>
    ),
  },
  {
    key: 'ready',
    label: 'Ready when this answers',
    hint: (
      <>
        A URL that answers once the product is up, with the port as <code>$PORT</code>.
      </>
    ),
  },
] as const

interface Props {
  draft: RunDraft
  onChange: (draft: RunDraft) => void
  startError?: string | undefined
  /** What is known of the commands: whether they started, and how they failed. */
  status?: ReactNode
  /** Why there is nothing to run, as the lead read it. */
  nothingReason?: string | undefined
}

/** Commands that start the product for each issue, or that there is nothing to run. */
export function RunChoice({ draft, onChange, startError, status, nothingReason }: Props) {
  const set = (part: Partial<RunDraft>): void => onChange({ ...draft, ...part })
  return (
    <RadioGroup
      legend="How the product runs"
      legendHidden
      value={draft.runs}
      onChange={(runs) => set({ runs })}
      options={[
        {
          value: 'commands',
          label: <b>It runs with these commands</b>,
          more: (
            <>
              {status !== undefined && <div aria-live="polite">{status}</div>}
              {FIELDS.map((f) => (
                <Field
                  key={f.key}
                  label={f.label}
                  optional={'optional' in f}
                  hint={f.hint}
                  error={f.key === 'start' ? startError : undefined}
                >
                  {(control) => (
                    <input
                      {...control}
                      className={controlClass}
                      spellCheck={false}
                      value={draft[f.key]}
                      onChange={(e) => set({ [f.key]: e.target.value })}
                    />
                  )}
                </Field>
              ))}
            </>
          ),
        },
        {
          value: 'nothing',
          label: <b>There is nothing to run</b>,
          hint: (
            <>
              {nothingReason !== undefined && (
                <>
                  The lead's reading: <q>{nothingReason}</q>{' '}
                </>
              )}
              Agents check their changes with the tests only, and Slack offers no preview.
            </>
          ),
        },
      ]}
    />
  )
}
