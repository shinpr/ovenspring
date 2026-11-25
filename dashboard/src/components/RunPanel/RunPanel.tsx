import Markdown from 'markdown-to-jsx'
import { useEffect, useRef } from 'react'
import type { LaunchDetail } from '../../api/types.gen.ts'
import { clock, duration, span } from '../../lib/time.ts'
import { CLI, PHASE, resultTone, runSentence } from '../../lib/vocabulary.ts'
import { Button } from '../Button/Button.tsx'
import { ExternalLink } from '../ExternalLink/ExternalLink.tsx'
import styles from './RunPanel.module.css'

interface Props {
  /** The run, once read; `null` when it could not be read. */
  run: LaunchDetail | null | undefined
  now: Date
  /** Where the run's raw output is served. */
  logUrl: string
  onClose: () => void
}

function cost(run: LaunchDetail): string {
  if (run.costUsd !== null) {
    return `$${run.costUsd.toFixed(2)}`
  }
  if (run.endedAt === null) {
    return 'Known when it ends'
  }
  return 'Not known'
}

function Facts({ run, now }: { run: LaunchDetail; now: Date }) {
  const tone = resultTone(run.result)
  return (
    <dl className={styles['facts']}>
      <dt>Agent</dt>
      <dd>
        {CLI[run.cli]} · {run.model ?? 'default model'} · effort {run.effort ?? 'default'}
      </dd>
      <dt>Started</dt>
      <dd>{clock(run.startedAt)}</dd>
      <dt>{run.endedAt === null ? 'Running for' : 'Took'}</dt>
      <dd>
        {run.endedAt === null
          ? `${duration(run.startedAt, now)} so far`
          : span(new Date(run.startedAt), new Date(run.endedAt))}
      </dd>
      <dt>Result</dt>
      <dd className={tone === 'muted' ? '' : styles[tone]}>
        {run.endedAt === null ? 'Running' : (runSentence(run).result ?? 'Done')}
      </dd>
      {run.failure !== null && (
        <>
          <dt>Why</dt>
          <dd>{run.failure}</dd>
        </>
      )}
      <dt>Cost</dt>
      <dd>{cost(run)}</dd>
    </dl>
  )
}

/** What an agent wrote, in Markdown; raw HTML in it stays text, since an agent wrote it. */
function Written({ text }: { text: string }) {
  return <Markdown options={{ disableParsingRawHTML: true, forceBlock: true }}>{text}</Markdown>
}

function Said({ run }: { run: LaunchDetail }) {
  const last = run.messages.at(-1)
  if (run.endedAt === null) {
    return run.messages.length === 0 ? (
      <p className={styles['muted']}>The agent has not said anything yet.</p>
    ) : (
      <ol className={styles['messages']}>
        {run.messages.map((m, i) => (
          // The messages only grow, in order, so a message's place is its identity.
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          <li key={i} className={styles['markdown']}>
            <Written text={m} />
          </li>
        ))}
      </ol>
    )
  }
  return last === undefined ? (
    <p className={styles['muted']}>The run ended before the agent said anything.</p>
  ) : (
    <div className={`${styles['said']} ${styles['markdown']}`}>
      <Written text={last} />
    </div>
  )
}

function sentence(run: LaunchDetail): string {
  const { who, what } = runSentence(run)
  return `${who} ${what}`
}

/** One agent run: how it ran, what the agent said, and its full log. */
export function RunPanel({ run, now, logUrl, onClose }: Props) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const d = dialog.current
    if (d !== null && !d.open) {
      d.showModal()
    }
  }, [])
  return (
    <dialog ref={dialog} className={styles['panel']} aria-labelledby="run-title" onClose={onClose}>
      <Button variant="quiet" className={styles['close']} onClick={() => dialog.current?.close()}>
        Close
      </Button>
      <p className={styles['muted']}>{run?.phase == null ? 'Run' : `${PHASE[run.phase]} · run`}</p>
      <h2 className={styles['title']} id="run-title">
        {run == null ? 'Run' : sentence(run)}
      </h2>
      {run === undefined && <p className={styles['muted']}>Reading the run…</p>}
      {run === null && <p className={styles['muted']}>This run could not be read.</p>}
      {run != null && (
        <>
          <Facts run={run} now={now} />
          <h3 className={styles['subheading']}>
            {run.endedAt === null ? 'What the agent has said so far' : 'What the agent said last'}
          </h3>
          <Said run={run} />
          <p className={styles['log']}>
            <ExternalLink href={logUrl}>Open the full log</ExternalLink>
            <span className={styles['muted']}>
              : everything {CLI[run.cli]} printed during the run.
            </span>
          </p>
        </>
      )}
    </dialog>
  )
}
