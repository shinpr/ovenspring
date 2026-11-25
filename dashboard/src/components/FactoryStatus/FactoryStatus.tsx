import type { ReactNode } from 'react'
import { Link } from 'react-router'
import type { Overview, Service, TrackerKind, Unreachable } from '../../api/types.gen.ts'
import { settingsPath } from '../../lib/settings.ts'
import { clock, dayAndClock } from '../../lib/time.ts'
import { typed } from '../../lib/typed.tsx'
import { asSentence } from '../../lib/vocabulary.ts'
import { ExternalLink } from '../ExternalLink/ExternalLink.tsx'
import styles from './FactoryStatus.module.css'

interface Props {
  overview: Overview
  /** When the last call to the factory failed, while calls fail. */
  lostAt?: Date | undefined
}

type Tone = 'halt' | 'yours' | 'plain'

interface Said {
  title: string
  tone: Tone
  lines: ReactNode
}

const TRACKER: Record<TrackerKind, string> = { linear: 'Linear', github: 'GitHub Issues' }

/** Where the open issues are, after "the 9 open issues": "on Linear", "in GitHub Issues". */
const WHERE: Record<TrackerKind, string> = { linear: 'on Linear', github: 'in GitHub Issues' }

function where(tracker: TrackerKind | null) {
  return tracker === null ? 'in the tracker' : WHERE[tracker]
}

/** A service out of reach: its name, what waits on it, and where its own status is. */
function outOfReach(service: Service, tracker: TrackerKind | null) {
  if (service === 'github') {
    return {
      name: 'GitHub',
      waits: 'Steps that need GitHub (pushing, CI, merging) wait. The rest go on.',
      status: ['https://www.githubstatus.com', 'githubstatus.com'],
    } as const
  }
  const name = tracker === null ? 'the issue tracker' : TRACKER[tracker]
  return {
    name,
    waits: 'Reading new issues and marking them started and done wait. The rest goes on.',
    status:
      tracker === 'linear'
        ? (['https://linearstatus.com', 'linearstatus.com'] as const)
        : (['https://www.githubstatus.com', 'githubstatus.com'] as const),
  }
}

const PART = {
  facts: "The analyzer is collecting the sprint's facts. The review then reaches you in Slack.",
  review: 'The lead is judging each key result. The review then reaches you in Slack.',
  agreements:
    'A reviewer is checking the proposed working agreements. The review then reaches you in Slack.',
} as const

/** What the link under a missing prerequisite says, by the setting that fixes it. */
const FIX_LINK: Record<string, string> = {
  slack: 'Change the Slack tokens in Settings',
  general: 'Change the GitHub token in Settings',
}

function fixLink(setting: string, tracker: TrackerKind | null) {
  if (setting === 'tracker') {
    return tracker === 'github'
      ? 'Change the issue tracker in Settings'
      : 'Change the Linear key in Settings'
  }
  return FIX_LINK[setting] ?? 'Change it in Settings'
}

function prerequisites(o: Overview): ReactNode {
  return (
    <>
      <p className={styles['line']}>Steps that need these wait. Everything else goes on.</p>
      <ul className={styles['missing']}>
        {o.status.prerequisites.map((m) => (
          <li key={m.what}>
            <strong>{typed(m.what)}</strong>
            <span className={styles['fix']}>{typed(m.fix)}</span>
            {m.setting != null && (
              <Link className={styles['fixLink']} to={settingsPath(m.setting)}>
                {fixLink(m.setting, o.tracker)}
              </Link>
            )}
          </li>
        ))}
      </ul>
    </>
  )
}

function working(o: Overview): Said {
  const running = o.issues.filter((i) => i.now.section === 'in-progress').length
  if (running === 0 && o.decisions.length > 0) {
    return { title: 'Waiting for you', tone: 'yours', lines: 'Everything under way waits on you.' }
  }
  return {
    title: 'Working',
    tone: 'plain',
    lines: `${running} ${running === 1 ? 'issue' : 'issues'} in progress.`,
  }
}

/** Where the lead said what is missing: a link to its message, with when it was sent. */
function told(o: Overview): ReactNode {
  const t = o.toldWaiting
  const words = 'the lead told you in Slack what is missing'
  if (t === null) {
    return `${words}.`
  }
  const when = ` (${dayAndClock(t.at)})`
  return t.link === null ? (
    `${words}${when}.`
  ) : (
    <>
      <ExternalLink href={t.link}>{words}</ExternalLink>
      {when}.
    </>
  )
}

/** The working agreements' pull request: what it waits on the owner for, if anything. */
function agreements(o: Overview): Said {
  const ask = o.decisions.find((d) => d.issue === null)?.ask
  if (ask === 'approve-on-github') {
    return {
      title: 'Waiting for you',
      tone: 'yours',
      lines:
        'Approve the working agreements and knowledge on GitHub. The factory then merges them and proposes the next sprint.',
    }
  }
  if (ask === 'merge-on-github') {
    return {
      title: 'Waiting for you',
      tone: 'yours',
      lines:
        'Merge the working agreements and knowledge on GitHub. The next sprint is proposed once they are merged.',
    }
  }
  return {
    title: 'Merging the working agreements',
    tone: 'plain',
    lines: (
      <>
        They merge once their checks pass, and the next sprint is proposed then. Nothing is needed
        from you.{' '}
        {o.agreements !== null && (
          <ExternalLink href={o.agreements}>See the pull request</ExternalLink>
        )}
      </>
    ),
  }
}

function openIssues(o: Overview): string {
  return `${o.openIssues} open ${o.openIssues === 1 ? 'issue' : 'issues'}`
}

/** Why calls fail, as the first line names it. */
const FAILS: Record<Unreachable['why'], string> = {
  'no-connection': 'no connection',
  'server-error': 'errors on its side',
  'rate-limited': 'it limits how often it is called',
}

/** What the owner does about it, by reason: their network, the service's status, or nothing. */
function remedy(u: Unreachable, name: string, status: readonly [string, string]): ReactNode {
  switch (u.why) {
    case 'no-connection':
      return "Nothing to do unless it lasts: check this computer's network."
    case 'server-error':
      return (
        <>
          Nothing to do unless it lasts: {name} shows its own state at{' '}
          <ExternalLink href={status[0]}>{status[1]}</ExternalLink>.
        </>
      )
    case 'rate-limited':
      return 'Nothing to do: calls go through again once the limit resets.'
  }
}

function unreachable(o: Overview): Said {
  const u = o.status.unreachable
  if (u === null) {
    return {
      title: "Can't reach a service",
      tone: 'halt',
      lines: 'The factory tries again every 30 seconds.',
    }
  }
  const { name, waits, status } = outOfReach(u.service, o.tracker)
  return {
    title: `Can't reach ${name}`,
    tone: 'halt',
    lines: (
      <>
        <p className={styles['line']}>
          Since {clock(u.since)} every call to {name} fails ({FAILS[u.why]}). The factory tries
          again every 30 seconds. {waits}
        </p>
        <p className={styles['line']}>{remedy(u, name, status)}</p>
      </>
    ),
  }
}

/** A pause reason as a sentence: the daemon writes it to follow "paused because", lowercase and
 * without a period, and an owner's `!pause` gives their own words. */
function sentence(reason: string): string {
  return asSentence(reason.charAt(0).toUpperCase() + reason.slice(1))
}

/** What the factory is doing, as the first state that holds says it. */
function said(o: Overview): Said {
  const s = o.status
  switch (s.state) {
    case 'paused':
      return {
        title: 'Paused',
        tone: 'halt',
        lines: (
          <>
            <p className={styles['line']}>
              {s.reason === null ? '' : `${sentence(s.reason)} `}Nothing new starts until you
              resume.
            </p>
            <p className={styles['line']}>
              Resume with <kbd>@{o.mention} !resume</kbd> in Slack
              {s.reason === null ? '.' : ' once its cause is fixed.'}
            </p>
          </>
        ),
      }
    case 'prerequisites':
      return { title: 'Partly on hold', tone: 'halt', lines: prerequisites(o) }
    case 'unreachable':
      return unreachable(o)
    case 'settings-not-applied':
      return {
        title: 'Settings not applied',
        tone: 'halt',
        lines: (
          <>
            <p className={styles['line']}>{s.configError}</p>
            <p className={styles['line']}>
              The factory goes on with the settings it last read without an error. Change it in{' '}
              <Link to="/settings">Settings</Link> or fix <code>.ovenspring/config.toml</code>.
            </p>
          </>
        ),
      }
    case 'proposing':
      return {
        title:
          o.lastSprintEndedAt === null ? 'Proposing the first sprint' : 'Proposing the next sprint',
        tone: 'plain',
        lines: `The lead is choosing issues from the goal and the ${openIssues(o)} ${where(o.tracker)}. The proposal will reach you in Slack.`,
      }
    case 'waiting-for-issues':
      return {
        title: 'Waiting for an issue that serves the goal',
        tone: 'plain',
        lines: (
          <>
            <p className={styles['line']}>
              {o.openIssues === 0
                ? `No issue is open ${where(o.tracker)};`
                : `None of the ${openIssues(o)} ${where(o.tracker)} moves the product toward the goal;`}{' '}
              {told(o)}
            </p>
            <p className={styles['line']}>
              A new sprint is proposed as soon as an issue opens {where(o.tracker)}.
            </p>
          </>
        ),
      }
    case 'proposal-with-owner':
      return {
        title: 'Waiting for you',
        tone: 'yours',
        lines: 'No sprint runs until you answer the proposal in Slack.',
      }
    case 'sprint-problem':
      return {
        title: 'The lead is handling a problem with the sprint',
        tone: 'plain',
        lines: 'Nothing new starts until it is past. You are asked in Slack if the lead needs you.',
      }
    case 'sprint-with-owner':
      return {
        title: 'Waiting for you',
        tone: 'yours',
        lines: 'The sprint goes on when you answer in Slack.',
      }
    case 'reviewing':
      return {
        title: 'Reviewing the sprint',
        tone: 'plain',
        lines: `Every issue has ended. ${s.reviewPart === null ? 'The review reaches you in Slack.' : PART[s.reviewPart]}`,
      }
    case 'review-with-owner':
      return {
        title: 'Waiting for you',
        tone: 'yours',
        lines:
          'Read the sprint review in Slack. Reply with what you think of the result, or close the sprint.',
      }
    case 'applying-agreements':
      return agreements(o)
    case 'working':
      return working(o)
  }
}

function lost(at: Date): Said {
  return {
    title: 'The dashboard lost contact with the factory',
    tone: 'halt',
    lines: (
      <>
        <p className={styles['line']}>
          Last contact {clock(at.toISOString())}. Everything below is as it was then.
        </p>
        <p className={styles['line']}>
          If the factory stopped, run <kbd>ovenspring start</kbd> in a terminal in this repository's
          folder. It opens a new dashboard tab.
        </p>
      </>
    ),
  }
}

/** The page title of Now: the factory's one state, and what the owner does about it. */
export function FactoryStatus({ overview, lostAt }: Props) {
  const { title, tone, lines } = lostAt === undefined ? said(overview) : lost(lostAt)
  return (
    <div className={styles['status']} role="status" aria-live="polite">
      <h1 className={`${styles['title']} ${styles[tone]}`}>{title}</h1>
      <div className={styles['lines']}>
        {typeof lines === 'string' ? <p className={styles['line']}>{lines}</p> : lines}
      </div>
    </div>
  )
}
