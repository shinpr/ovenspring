import type { ReactNode } from 'react'
import { Link } from 'react-router'
import type { IssueRow, Waiting } from '../../api/types.gen.ts'
import { duration, when } from '../../lib/time.ts'
import {
  ASK,
  activityWords,
  agentAtWork,
  ENDING,
  failure,
  PHASE,
  problemWords,
} from '../../lib/vocabulary.ts'
import { ExternalLink } from '../ExternalLink/ExternalLink.tsx'
import styles from './Glance.module.css'

interface SectionProps {
  title: string
  /** Said when the section holds nothing; the section is left out when not given. */
  empty?: string
  count: number
  children: ReactNode
}

/** One section of Now: its heading and its rows. */
export function GlanceSection({ title, empty, count, children }: SectionProps) {
  if (count === 0 && empty === undefined) {
    return null
  }
  return (
    <section className={styles['section']} aria-label={title}>
      <h2 className={styles['heading']}>{title}</h2>
      {count === 0 ? (
        <p className={styles['empty']}>{empty}</p>
      ) : (
        <ul className={styles['rows']}>{children}</ul>
      )}
    </section>
  )
}

interface RowProps {
  issueKey: string | null
  title: string
  phase?: string
  status: ReactNode
  meta?: string
}

function Row({ issueKey, title, phase = '', status, meta = '' }: RowProps) {
  return (
    <li className={styles['row']}>
      <span className={styles['key']}>
        {issueKey !== null && <Link to={`/issues/${issueKey}`}>{issueKey}</Link>}
      </span>
      <span className={styles['title']}>
        {issueKey === null ? title : <Link to={`/issues/${issueKey}`}>{title}</Link>}
      </span>
      <span className={styles['phase']}>{phase}</span>
      <span className={styles['status']}>{status}</span>
      <span className={styles['meta']}>{meta}</span>
    </li>
  )
}

/** One decision the owner has: the action opens where they take it. */
export function DecisionRow({ waiting }: { waiting: Waiting }) {
  const where =
    waiting.ask === 'merge-on-github' || waiting.ask === 'approve-on-github'
      ? 'on GitHub'
      : 'in Slack'
  const status =
    waiting.link === null ? (
      <span className={styles['muted']}>{ASK[waiting.ask]} · not in Slack yet</span>
    ) : (
      <ExternalLink className={styles['yours']} href={waiting.link} place={where}>
        <span aria-hidden="true">● </span>
        {ASK[waiting.ask]}
      </ExternalLink>
    )
  return <Row issueKey={waiting.issue} title={waiting.title} status={status} />
}

interface IssueProps {
  issue: IssueRow
  now: Date
  /** Opens the run of an issue whose step is running. */
  onFollow?: ((key: string) => void) | undefined
}

function progressStatus({ issue, onFollow }: IssueProps): ReactNode {
  const p = issue.now
  if (p.onHold) {
    return <span className={styles['muted']}>On hold (paused)</span>
  }
  const problem = failure(p)
  if (problem !== null) {
    return (
      <span className={styles['problem']}>
        <span aria-hidden="true">▲ </span>Problem: {problemWords(problem)}
      </span>
    )
  }
  const words = activityWords(p)
  return agentAtWork(p) && onFollow !== undefined ? (
    <button type="button" className={styles['follow']} onClick={() => onFollow(issue.key)}>
      {words}
    </button>
  ) : (
    words
  )
}

/** An issue in progress: its phase, what it does, for how long. */
export function ProgressRow(props: IssueProps) {
  const { issue, now } = props
  return (
    <Row
      issueKey={issue.key}
      title={issue.title}
      phase={issue.now.phase === null ? '' : PHASE[issue.now.phase]}
      status={progressStatus(props)}
      meta={issue.now.since === null ? '' : duration(issue.now.since, now)}
    />
  )
}

/** An issue waiting to start, and why. */
export function QueuedRow({ issue }: { issue: IssueRow }) {
  const wait = issue.now.wait
  let status: ReactNode = ''
  if (wait?.reason === 'after') {
    const [first, ...rest] = wait.keys
    status = (
      <span className={styles['muted']}>
        After <Link to={`/issues/${first ?? ''}`}>{first}</Link>
        {rest.length === 0 ? ' merges' : ` and ${rest.length} more merge`}
      </span>
    )
  } else if (wait?.reason === 'place') {
    status = <span className={styles['muted']}>When an issue in progress ends</span>
  }
  return <Row issueKey={issue.key} title={issue.title} status={status} />
}

/** An issue that ended: how, and when; a sprint can span days, so an earlier day shows its date. */
export function DoneRow({ issue, now }: { issue: IssueRow; now: Date }) {
  const { ended, endedAt } = issue.now
  return (
    <Row
      issueKey={issue.key}
      title={issue.title}
      status={
        ended === null ? (
          ''
        ) : (
          <span className={ended === 'merged' ? '' : styles['muted']}>{ENDING[ended]}</span>
        )
      }
      meta={endedAt === null ? '' : when(endedAt, now)}
    />
  )
}
