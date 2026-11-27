import type { ReactNode } from 'react'
import { Link } from 'react-router'
import type {
  EventIssue,
  EventSprint,
  EventLine as Line,
  NoticeState,
} from '../../api/types.gen.ts'
import { ExternalLink } from '../../components/ExternalLink/ExternalLink.tsx'
import { clock, clockOn } from '../../lib/time.ts'
import { unmarked } from '../../lib/typed.tsx'
import { eventSentence, type Part, trackerLinkName } from '../../lib/vocabulary.ts'
import styles from './Events.module.css'

/** A sentence's parts as the page shows them: commands in the mono face, words in quotes. */
export function Words({ parts }: { parts: readonly Part[] }) {
  const [first, ...rest] = parts
  return first === undefined ? null : (
    <>
      <Shown part={first} />
      <Words parts={rest} />
    </>
  )
}

function Shown({ part }: { part: Part }): ReactNode {
  if (typeof part === 'string') {
    return part
  }
  if ('code' in part) {
    return <code>{part.code}</code>
  }
  if ('quote' in part) {
    return <q>{part.quote}</q>
  }
  return <Link to="/settings/connections">{part.settings}</Link>
}

/** An issue key: a link to its page when the factory has a record of it, plain text otherwise. */
export function IssueKey({ issue }: { issue: EventIssue }) {
  return issue.known ? <Link to={`/issues/${issue.key}`}>{issue.key}</Link> : issue.key
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** A sprint by its purpose, or by when it was begun, which its id records: "YYYYMMDD-HHMM…". */
function sprintName(sprint: EventSprint): string {
  if (sprint.purpose !== null) {
    return `Sprint: ${unmarked(sprint.purpose)}`
  }
  const [, month, date, hour, minute] = /^\d{4}(\d{2})(\d{2})-(\d{2})(\d{2})/.exec(sprint.id) ?? []
  const named = MONTHS[Number(month) - 1]
  return named === undefined
    ? `Sprint ${sprint.id}`
    : `Sprint begun ${named} ${Number(date)}, ${hour}:${minute}`
}

function Subject({ line }: { line: Line }) {
  if (line.issue !== null) {
    return <IssueKey issue={line.issue} />
  }
  if (line.sprint === null) {
    return null
  }
  const name = sprintName(line.sprint)
  return line.sprint.listed ? (
    <Link to={`/sprints#sprint-${line.sprint.id}`} aria-label={name} title={name}>
      Sprint
    </Link>
  ) : (
    <span className={styles['sprintName']}>{name}</span>
  )
}

/** Whether the owner heard of this start or clear. */
function toldWords(told: NoticeState, at: string): string {
  switch (told.state) {
    case 'told': {
      const when = clockOn(told.at, at)
      return when === clock(at) ? 'You were told in Slack.' : `You were told in Slack at ${when}.`
    }
    case 'waiting':
      return 'Not told yet: Slack did not accept the message. It is sent once Slack accepts messages again.'
    case 'off':
      return 'Not told: error notices are off.'
  }
}

/** What the line's second line says: how often its error happened, and who was told. */
function after(line: Line): string[] {
  const e = line.error
  if (e === null) {
    return []
  }
  switch (e.role) {
    case 'started':
      return [
        ...(e.times > 1
          ? [
              `Happened ${e.times} times${e.open ? ' so far' : ''}, last at ${clockOn(e.last, line.at)}.`,
            ]
          : []),
        toldWords(e.told, line.at),
      ]
    case 'repeat':
      return [
        ...(e.times > 1 ? [`Happened ${e.times} times, last at ${clockOn(e.last, line.at)}.`] : []),
        `Same error as at ${clockOn(e.startedAt, line.at)}, so no new message.`,
      ]
    case 'cleared':
      return [toldWords(e.told, line.at)]
  }
}

/** One recorded line: clock · the issue or sprint it concerns · what happened. */
export function EventLine({ line }: { line: Line }) {
  const role = line.error?.role
  const url = line.kind === 'filter-dropped' ? line.fields['url'] : undefined
  const second = after(line)
  return (
    <li className={styles['line']} id={`event-${line.seq}`} tabIndex={-1}>
      <span className={styles['clock']}>{clock(line.at)}</span>
      <span className={styles['subject']}>
        <Subject line={line} />
      </span>
      <span className={styles['sentence']}>
        {role === 'started' || role === 'repeat' ? (
          <span className={`${styles['word']} ${styles['fail']}`}>Error</span>
        ) : null}
        {role === 'cleared' ? (
          <span className={`${styles['word']} ${styles['ok']}`}>Cleared</span>
        ) : null}
        {role === undefined ? null : ' '}
        <Words parts={eventSentence(line)} />
        {(second.length > 0 || typeof url === 'string') && (
          <span className={styles['after']}>
            {second.join(' ')}
            {typeof url === 'string' && (
              <>
                {second.length > 0 && ' '}
                <ExternalLink href={url}>{trackerLinkName(url)}</ExternalLink>
              </>
            )}
          </span>
        )}
      </span>
    </li>
  )
}
