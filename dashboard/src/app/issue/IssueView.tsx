import { type ReactNode, useState } from 'react'
import { Link } from 'react-router'
import type { IssueDetail, LaunchRow, PhaseSpan } from '../../api/types.gen.ts'
import { Breadcrumb } from '../../components/Breadcrumb/Breadcrumb.tsx'
import { Button } from '../../components/Button/Button.tsx'
import { CancelPanel } from '../../components/CancelPanel/CancelPanel.tsx'
import { DownloadPanel } from '../../components/DownloadPanel/DownloadPanel.tsx'
import { ExternalLink } from '../../components/ExternalLink/ExternalLink.tsx'
import { RunLine } from '../../components/RunLine/RunLine.tsx'
import { settingsPath } from '../../lib/settings.ts'
import { clock, duration } from '../../lib/time.ts'
import {
  ASK,
  activityWords,
  asSentence,
  ENDING,
  nextInPhase,
  PHASE,
  phaseHolds,
  phaseSkipped,
  problemWords,
  ROLE,
  SCALE,
  STEP_NAME,
} from '../../lib/vocabulary.ts'
import appStyles from '../App.module.css'
import styles from './Issue.module.css'

interface Props {
  issue: IssueDetail
  /** The issue's runs, newest first. */
  runs: readonly LaunchRow[]
  now: Date
  onOpenRun: (id: string) => void
  onDownload: () => Promise<void>
  onCancel: (words: string) => Promise<void>
}

/** What the agent under way is doing, as the rest of a sentence that starts with its role. */
function doing(p: IssueDetail['now']): string {
  if (p.task !== null) {
    return `is on task ${p.task.number} of ${p.task.of}`
  }
  const words = activityWords(p)
  return words === '' ? 'is at work' : `is ${words.charAt(0).toLowerCase()}${words.slice(1)}`
}

interface Follow {
  /** The run under way, whose agent the line names and the owner can follow. */
  running: LaunchRow | undefined
  onOpenRun: (id: string) => void
}

function inProgress(issue: IssueDetail, now: Date, { running, onOpenRun }: Follow): ReactNode {
  const p = issue.now
  if (p.problem !== null) {
    return (
      <span className={styles['fail']}>
        The lead is handling a problem: {problemWords(p.problem)}
      </span>
    )
  }
  if (p.activity === 'blocked-on-git-hub') {
    return (
      <>
        GitHub won't merge it yet.{' '}
        {issue.pullRequest !== null && (
          <ExternalLink href={issue.pullRequest}>See why on the pull request</ExternalLink>
        )}
      </>
    )
  }
  // Several reviews run at once: the path below names each one and its agent.
  const one = p.activity === 'reviews-running' ? undefined : running
  return (
    <>
      {one === undefined ? (
        activityWords(p)
      ) : (
        <>
          <b>{ROLE[one.role]}</b> {doing(p)}
        </>
      )}
      {p.onHold && <span className={styles['muted']}> · on hold while the factory is paused</span>}
      {!p.onHold && p.since !== null && (
        <span className={styles['muted']}> · for {duration(p.since, now)}</span>
      )}
      {one !== undefined && (
        <>
          {' '}
          <button type="button" className={styles['follow']} onClick={() => onOpenRun(one.id)}>
            follow
          </button>
        </>
      )}
    </>
  )
}

function waiting(issue: IssueDetail): ReactNode {
  const wait = issue.now.wait
  if (wait?.reason === 'after') {
    return (
      <>
        Starts after {wait.keys.join(' and ')} {wait.keys.length === 1 ? 'merges' : 'merge'}.
      </>
    )
  }
  if (wait?.reason === 'place') {
    const which = wait.heldBy.length === 1 ? wait.heldBy[0] : `one of ${wait.heldBy.join(', ')}`
    return (
      <>
        Starts when {which} ends, because <Link to={settingsPath('limits')}>Settings</Link> limits
        how many issues run at once.
      </>
    )
  }
  return 'Waiting to start.'
}

/** Where the issue is now, in one line. */
function NowLine({ issue, now, follow }: { issue: IssueDetail; now: Date; follow: Follow }) {
  const p = issue.now
  let text: ReactNode
  switch (p.section) {
    case 'in-progress':
      text = inProgress(issue, now, follow)
      break
    case 'you':
      text = (
        <>
          <b className={styles['accent']}>
            Waiting for you: {issue.decision === null ? 'your answer' : ASK[issue.decision.ask]}
          </b>{' '}
          {issue.decision?.link != null && (
            <ExternalLink href={issue.decision.link}>
              {issue.decision.ask === 'merge-on-github'
                ? 'Open the pull request'
                : 'Answer in Slack'}
            </ExternalLink>
          )}
        </>
      )
      break
    case 'waiting-to-start':
      text = waiting(issue)
      break
    case 'done':
      text = asSentence(
        `${p.ended === null ? 'Ended' : ENDING[p.ended]}${p.endedAt === null ? '' : ` at ${clock(p.endedAt)}`}${issue.canceled ? `: ${issue.canceled}` : ''}`
      )
      break
  }
  return <p className={styles['now']}>{text}</p>
}

function spanText(phase: PhaseSpan): string {
  if (phase.startedAt === null) {
    return ''
  }
  const from = clock(phase.startedAt)
  if (phase.state === 'now') {
    return ` · from ${from}`
  }
  return phase.endedAt === null ? ` · ${from}–` : ` · ${from}–${clock(phase.endedAt)}`
}

interface PhaseProps {
  phase: PhaseSpan
  issue: IssueDetail
  runs: readonly LaunchRow[]
  now: Date
  onOpenRun: (id: string) => void
}

function PhaseItem({ phase, issue, runs, now, onOpenRun }: PhaseProps) {
  const mine = [...runs.filter((r) => r.phase === phase.phase)].reverse()
  const list = (
    <ul className={styles['runs']}>
      {mine.map((r) => (
        <RunLine key={r.id} run={r} now={now} onOpen={onOpenRun} place="path" />
      ))}
    </ul>
  )
  const next = phase.state === 'now' ? nextInPhase(issue.now.activity) : undefined
  return (
    <li className={`${styles['phase']} ${phase.state === 'now' ? styles['current'] : ''}`}>
      <div className={styles['phaseHead']}>
        <h3 className={styles['phaseName']}>{PHASE[phase.phase]}</h3>
        <span className={styles['state']}>
          {phase.state}
          {spanText(phase)}
        </span>
      </div>
      {phase.state === 'ahead' && <p className={styles['desc']}>{phaseHolds(phase.phase)}</p>}
      {phase.state === 'skipped' && (
        <p className={styles['desc']}>{phaseSkipped(phase.phase, issue.scale, issue.now.ended)}</p>
      )}
      {phase.state === 'now' && mine.length > 0 && list}
      {next !== undefined && <p className={styles['desc']}>Next: {next}</p>}
      {phase.state === 'done' && mine.length > 0 && (
        <details className={styles['runsOf']}>
          <summary>
            {mine.length} {mine.length === 1 ? 'run' : 'runs'}
          </summary>
          {list}
        </details>
      )}
    </li>
  )
}

const TONE = { apply: 'ok', decline: 'muted' } as const

function toneOf(disposition: IssueDetail['findings'][number]['disposition']) {
  return disposition === null ? 'accent' : TONE[disposition]
}

function Findings({ issue }: { issue: IssueDetail }) {
  const all = issue.findings
  if (all.length === 0) {
    return <p className={styles['muted']}>No findings so far.</p>
  }
  const applied = all.filter((f) => f.disposition === 'apply').length
  const declined = all.filter((f) => f.disposition === 'decline').length
  const open = all.length - applied - declined
  const parts = [
    applied > 0 && `${applied} applied`,
    declined > 0 && `${declined} declined`,
    open > 0 && `${open} being judged`,
  ].filter(Boolean)
  return (
    <>
      <p className={styles['prose']}>
        {all.length} {all.length === 1 ? 'finding' : 'findings'}: {parts.join(', ')}.
      </p>
      <details className={styles['runsOf']}>
        <summary>Show each finding and its answer</summary>
        <ul className={styles['findings']}>
          {all.map((f, i) => (
            // A finding has no id of its own across reviews; its place in the list is stable.
            // biome-ignore lint/suspicious/noArrayIndexKey: see above
            <li key={i}>
              <span className={`${styles['disposition']} ${styles[toneOf(f.disposition)]}`}>
                {f.disposition === null
                  ? 'being judged'
                  : { apply: 'applied', decline: 'declined' }[f.disposition]}
              </span>{' '}
              <span className={styles['muted']}>
                ·{' '}
                {f.source.kind === 'review'
                  ? `${STEP_NAME[f.source.review]}, round ${f.source.round}`
                  : `${f.source.author} on the pull request`}
              </span>
              <p className={styles['prose']}>{f.text}</p>
              {f.answer !== null && <p className={styles['muted']}>{f.answer}</p>}
            </li>
          ))}
        </ul>
      </details>
    </>
  )
}

/** "22 runs, 1 running. $3.14 so far; 2 have no known cost." */
function cost(runs: readonly LaunchRow[]): string {
  const ended = runs.filter((r) => r.result !== 'running')
  const running = runs.length - ended.length
  const count = `${runs.length} ${runs.length === 1 ? 'run' : 'runs'}${running > 0 ? `, ${running} running` : ''}.`
  const costed = ended.filter((r) => r.costUsd !== null)
  const uncosted = ended.length - costed.length
  const spent = costed.reduce((sum, r) => sum + (r.costUsd ?? 0), 0)
  const parts = [
    costed.length > 0 ? `$${spent.toFixed(2)} so far` : '',
    uncosted > 0 ? `${uncosted} ${uncosted === 1 ? 'has' : 'have'} no known cost` : '',
  ].filter(Boolean)
  return parts.length === 0 ? count : `${count} ${parts.join('; ')}.`
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  const id = `group-${title.toLowerCase().replaceAll(' ', '-')}`
  return (
    <section className={styles['group']} aria-labelledby={id}>
      <h2 className={styles['heading']} id={id}>
        {title}
      </h2>
      {children}
    </section>
  )
}

const TRACKERS: ReadonlyArray<readonly [domain: string, link: string, name: string]> = [
  ['linear.app', 'Linear issue', 'Linear'],
  ['github.com', 'GitHub issue', 'GitHub'],
]

function trackerOf(url: string) {
  const host = new URL(url).hostname
  return TRACKERS.find(([domain]) => host.endsWith(domain))
}

function trackerLink(url: string) {
  return trackerOf(url)?.[1] ?? 'Issue'
}

/** Where the issue lives, as the owner calls it. */
function trackerName(url: string | null) {
  return (url === null ? undefined : trackerOf(url)?.[2]) ?? 'the tracker'
}

/** One issue: where it is, the path it takes, what it is, what reviews found, and its runs. */
export function IssueView({ issue, runs, now, onOpenRun, onDownload, onCancel }: Props) {
  const [canceling, setCanceling] = useState(false)
  // An issue that ended held still has work the owner can stop; any other end has none.
  const cancelable = issue.now.ended === null || issue.now.ended === 'held'
  return (
    <>
      <Breadcrumb>{issue.key}</Breadcrumb>
      <div className={styles['head']}>
        <h1 className={appStyles['title']}>{issue.title}</h1>
        {cancelable && (
          <Button
            className={styles['cancel']}
            aria-expanded={canceling}
            aria-controls="cancel-issue"
            onClick={() => setCanceling(!canceling)}
          >
            Cancel issue…
          </Button>
        )}
      </div>
      <p className={styles['links']}>
        {issue.tracker !== null && (
          <ExternalLink href={issue.tracker}>{trackerLink(issue.tracker)}</ExternalLink>
        )}
        {issue.pullRequest === null ? (
          <span className={styles['muted']}>No pull request yet</span>
        ) : (
          <ExternalLink href={issue.pullRequest}>
            {`Pull request #${issue.pullRequest.split('/').pop() ?? ''}`}
          </ExternalLink>
        )}
        {issue.prototype !== null && <Link to={`/issues/${issue.key}/prototype`}>Prototype</Link>}
      </p>
      {cancelable && (
        <CancelPanel
          id="cancel-issue"
          hidden={!canceling}
          tracker={trackerName(issue.tracker)}
          onCancel={onCancel}
        />
      )}
      <NowLine
        issue={issue}
        now={now}
        follow={{ running: runs.find((r) => r.result === 'running'), onOpenRun }}
      />
      <div className={styles['region']}>
        <Group title="Path">
          <ol className={styles['path']}>
            {issue.path.map((p) => (
              <PhaseItem
                key={p.phase}
                phase={p}
                issue={issue}
                runs={runs}
                now={now}
                onOpenRun={onOpenRun}
              />
            ))}
          </ol>
        </Group>
        <Group title="What it is">
          <dl className={styles['whatis']}>
            {issue.outcome !== null && (
              <>
                <dt>Outcome</dt>
                <dd>{issue.outcome}</dd>
              </>
            )}
            {issue.nonGoals.length > 0 && (
              <>
                <dt>Not in it</dt>
                <dd>
                  {issue.nonGoals.map((g) => (
                    <span key={g} className={styles['line']}>
                      {g}
                    </span>
                  ))}
                </dd>
              </>
            )}
            <dt>Size</dt>
            <dd>
              {issue.scale === null ? (
                <span className={styles['muted']}>
                  {issue.now.section === 'done'
                    ? 'Not judged: the issue ended first.'
                    : 'Judged when the lead reads the issue.'}
                </span>
              ) : (
                <b>{SCALE[issue.scale]}</b>
              )}
            </dd>
          </dl>
        </Group>
        <Group title="Review findings">
          <Findings issue={issue} />
        </Group>
        <Group title="Runs and cost">
          {runs.length === 0 ? (
            <p className={styles['prose']}>No runs yet.</p>
          ) : (
            <>
              <p className={styles['prose']}>{cost(runs)}</p>
              <p className={styles['links']}>
                <Link to={`/activity?issue=${encodeURIComponent(issue.key)}`}>
                  See them in Activity
                </Link>
              </p>
            </>
          )}
          <DownloadPanel what="this issue's records" onDownload={onDownload} />
        </Group>
      </div>
    </>
  )
}
