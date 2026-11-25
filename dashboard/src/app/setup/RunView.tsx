import { type FormEvent, useState } from 'react'
import { ApiError } from '../../api/client.ts'
import type { RunCommandsOutput, RunRequest, SetupRun } from '../../api/types.gen.ts'
import { Button } from '../../components/Button/Button.tsx'
import {
  RunChoice,
  type RunDraft,
  runDraft,
  runRequest,
} from '../../components/RunChoice/RunChoice.tsx'
import { seconds } from '../../lib/time.ts'
import styles from './Setup.module.css'

interface Props {
  run: SetupRun
  /** Settles how the product runs; a refusal carries `check` (the failed start) or `start`. */
  onSave: (request: RunRequest) => Promise<void>
}

/** Run the product: the lead finds how it starts, the factory starts it once, the owner decides. */
export function RunView({ run, onSave }: Props) {
  return (
    <>
      <div className={styles['intro']}>
        <h2 className={styles['stepTitle']}>Run the product</h2>
        <p className={styles['text']}>
          Each issue gets its own running copy of the product, for agents to check their changes and
          for you to preview. Confirm how it starts, or say there is nothing to run.
        </p>
      </div>
      <Body run={run} onSave={onSave} />
    </>
  )
}

function Body({ run, onSave }: Props) {
  if (run.found !== null && !run.checking) {
    return <Decide key={JSON.stringify(run.found)} run={run} found={run.found} onSave={onSave} />
  }
  if (run.found === null && run.stopped !== null) {
    return <Stopped reason={run.stopped} mention={run.mention} />
  }
  return <Finding run={run} />
}

const MARKS = {
  done: ['✓', 'ok'],
  now: ['●', 'accent'],
  ahead: ['○', 'muted'],
} as const

function Mark({ state }: { state: 'done' | 'now' | 'ahead' }) {
  const [glyph, tone] = MARKS[state]
  return (
    <span className={styles[tone]} aria-hidden="true">
      {glyph}
    </span>
  )
}

/** While the lead reads and the factory starts the product: where it is, and what the lead said. */
function Finding({ run }: { run: SetupRun }) {
  const reading = run.found === null
  return (
    <>
      <ol className={styles['progress']} aria-label="Progress" aria-live="polite">
        <li>
          <Mark state={reading ? 'now' : 'done'} />
          <span>
            {reading ? (
              <strong>The lead is reading the repository</strong>
            ) : (
              'The lead found the commands'
            )}
          </span>
        </li>
        <li>
          <Mark state={reading ? 'ahead' : 'now'} />
          <span>
            {reading ? 'Start it once to check' : <strong>Starting it once to check</strong>}
          </span>
          {!reading && run.found?.start != null && (
            <span className={styles['sub']}>
              <code>{run.found.start}</code>
              {run.found.ready != null && (
                <>
                  , waiting for <code>{run.found.ready}</code> to answer
                </>
              )}
            </span>
          )}
        </li>
      </ol>
      {reading && (
        <section aria-labelledby="setup-said">
          <h3 className={styles['blockTitle']} id="setup-said">
            What the lead has said so far
          </h3>
          {run.said.length === 0 ? (
            <p className={`${styles['small']} ${styles['muted']}`}>Nothing yet.</p>
          ) : (
            <ul className={styles['said']}>
              {run.said.map((m, i) => (
                // The messages only grow, in order, so their position names them.
                // biome-ignore lint/suspicious/noArrayIndexKey: see above
                <li key={i}>{m}</li>
              ))}
            </ul>
          )}
        </section>
      )}
      <p className={`${styles['small']} ${styles['muted']}`}>
        This takes a few minutes. Closing this page does not stop it.
      </p>
    </>
  )
}

/** The lead's agent failed to run: its own error, and how the factory goes on. */
function Stopped({ reason, mention }: { reason: string; mention: string }) {
  return (
    <div role="status">
      <p className={styles['text']}>
        <span className={styles['fail']} aria-hidden="true">
          ✗
        </span>{' '}
        <strong className={styles['fail']}>The lead's agent stopped with an error.</strong> What it
        wrote:
      </p>
      <pre className={styles['out']}>{reason}</pre>
      <p className={styles['small']}>
        The factory is paused. When it can run, resume with <kbd>@{mention} !resume</kbd> in Slack,
        and the lead reads the repository again.
      </p>
    </div>
  )
}

/** The lead's answer and the start's result, for the owner to keep, change or replace. */
function Decide({
  run,
  found,
  onSave,
}: {
  run: SetupRun
  found: RunCommandsOutput
  onSave: Props['onSave']
}) {
  const [draft, setDraft] = useState<RunDraft>(() => runDraft(found, found.runs === 'nothing'))
  const [edited, setEdited] = useState(false)
  const [failed, setFailed] = useState<string | undefined>()
  const [startError, setStartError] = useState<string | undefined>()
  const [notSaved, setNotSaved] = useState<string | undefined>()
  const [saving, setSaving] = useState(false)

  const save = async (event: FormEvent): Promise<void> => {
    event.preventDefault()
    if (draft.runs === 'commands' && draft.start.trim() === '') {
      setStartError('Enter the command that starts the product.')
      return
    }
    setSaving(true)
    setNotSaved(undefined)
    try {
      await onSave(runRequest(draft))
    } catch (e) {
      const r = refusal(e)
      setStartError(r.start)
      setFailed(r.check)
      setNotSaved(r.notSaved)
      if (r.check !== undefined) {
        setEdited(false)
      }
    } finally {
      setSaving(false)
    }
  }

  const status = (() => {
    if (saving) {
      return (
        <p className={styles['small']}>
          <span className={styles['accent']} aria-hidden="true">
            ●
          </span>{' '}
          <strong>Starting it once to check</strong>{' '}
          <span className={styles['muted']}>
            · Prepare, then Start, then waiting for the ready address
          </span>
        </p>
      )
    }
    if (edited) {
      return (
        <p className={`${styles['small']} ${styles['muted']}`}>
          Changed. The factory starts it once when you continue.
        </p>
      )
    }
    const output = failed ?? failedOutput(run)
    if (output !== undefined) {
      return (
        <>
          <p className={styles['small']}>
            <span className={styles['fail']} aria-hidden="true">
              ✗
            </span>{' '}
            <strong className={styles['fail']}>Did not start.</strong> Fix what it needs on this
            computer or change the commands, then Continue.
          </p>
          <pre className={styles['out']}>{output}</pre>
        </>
      )
    }
    if (found.runs === 'unknown') {
      return (
        <p className={styles['small']}>
          <span className={styles['fail']} aria-hidden="true">
            ✗
          </span>{' '}
          <strong className={styles['fail']}>The lead could not tell how it starts.</strong>{' '}
          {found.reason} Enter the commands.
        </p>
      )
    }
    const started = run.check !== null && 'started' in run.check ? run.check.started : undefined
    return started === undefined ? null : (
      <p className={styles['small']}>
        <span className={styles['ok']} aria-hidden="true">
          ✓
        </span>{' '}
        Started on port {started.port} and answered after {seconds(started.seconds)}.
      </p>
    )
  })()

  return (
    <form
      className={styles['frame']}
      noValidate
      aria-label="How the product runs"
      onSubmit={(e) => {
        save(e).catch(() => undefined)
      }}
    >
      <RunChoice
        draft={draft}
        onChange={(d) => {
          setDraft(d)
          setStartError(undefined)
          if (d.setup !== draft.setup || d.start !== draft.start || d.ready !== draft.ready) {
            setEdited(true)
            setFailed(undefined)
          }
        }}
        startError={startError}
        status={status}
        nothingReason={found.runs === 'nothing' ? found.reason : undefined}
      />
      <div className={styles['actions']}>
        <Button variant="primary" type="submit" disabled={saving}>
          {saving ? 'Checking…' : 'Continue'}
        </Button>
      </div>
      {notSaved !== undefined && (
        <p className={styles['small']} role="alert">
          {notSaved} Try again.
        </p>
      )}
    </form>
  )
}

/** Why a save was refused: only a start that ran and failed is "did not start"; a refused start
 * command is the field's; anything else is the request's. */
function refusal(e: unknown): {
  start: string | undefined
  check: string | undefined
  notSaved: string | undefined
} {
  const errors = e instanceof ApiError ? e.errors : {}
  const start = errors['start']
  const check = errors['check']
  const notSaved =
    check === undefined && start === undefined
      ? (errors['request'] ?? 'It could not be saved: the factory did not answer.')
      : undefined
  return { start, check, notSaved }
}

function failedOutput(run: SetupRun): string | undefined {
  return run.check !== null && 'failed' in run.check ? run.check.failed.output : undefined
}
