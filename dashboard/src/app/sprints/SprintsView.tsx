import { Link } from 'react-router'
import type { EndedSprint, LaunchRow, Role, SprintLine } from '../../api/types.gen.ts'
import { DownloadPanel } from '../../components/DownloadPanel/DownloadPanel.tsx'
import { dayAndClock, days } from '../../lib/time.ts'
import { written } from '../../lib/typed.tsx'
import { CLI, ROLE } from '../../lib/vocabulary.ts'
import appStyles from '../App.module.css'
import styles from './Sprints.module.css'

interface Props {
  current: SprintLine | null
  /** The running sprint's runs. */
  runs: readonly LaunchRow[]
  ended: readonly EndedSprint[]
  onDownload: (id: string) => Promise<void>
}

const ROLES: readonly Role[] = ['lead', 'executor', 'reviewer', 'analyzer']

function money(usd: number): string {
  return `$${usd.toFixed(2)}`
}

function RoleCost({ role, runs }: { role: Role; runs: readonly LaunchRow[] }) {
  const clis = [...new Set(runs.map((r) => CLI[r.cli]))].join(' and ')
  const costed = runs.filter((r) => r.costUsd !== null)
  const spent = costed.reduce((sum, r) => sum + (r.costUsd ?? 0), 0)
  return (
    <>
      <dt>{ROLE[role]}</dt>
      <dd>
        {runs.length} {runs.length === 1 ? 'run' : 'runs'} on {clis} ·{' '}
        {costed.length === 0 ? (
          <span className={styles['muted']}>no known cost</span>
        ) : (
          money(spent)
        )}
      </dd>
    </>
  )
}

function Ended({ sprint, onDownload }: { sprint: EndedSprint; onDownload: Props['onDownload'] }) {
  const met = sprint.keyResults.filter((k) => k.met).length
  return (
    <details className={styles['ended']}>
      <summary>
        <span className={styles['range']}>{days(sprint.startedAt, sprint.endedAt)}</span>
        <span>{written(sprint.purpose)}</span>
        <span className={styles['met']}>
          {met} of {sprint.keyResults.length} key results met
        </span>
      </summary>
      <div className={styles['endedBody']}>
        <ul className={styles['results']}>
          {sprint.keyResults.map((k) => (
            <li key={k.text}>
              <b className={k.met ? styles['ok'] : styles['fail']}>{k.met ? 'Met' : 'Not met'}</b>{' '}
              {written(k.text)}
              <div className={styles['muted']}>{written(k.evidence)}</div>
            </li>
          ))}
        </ul>
        <div className={styles['facts']}>
          <span>{sprint.merged.length} merged</span>
          <span>{sprint.dropped.length} dropped</span>
          <span>{sprint.held.length} held</span>
        </div>
        <div className={`${styles['facts']} ${styles['muted']}`}>
          <span>
            Working agreements changed:{' '}
            {sprint.agreements.length === 0
              ? 'none'
              : sprint.agreements.map((a, i) => (
                  <span key={a}>
                    {i > 0 && ', '}
                    <code>{a}</code>
                  </span>
                ))}
          </span>
          <span>
            {sprint.launches} runs · {money(sprint.costUsd)}
            {sprint.uncosted > 0 && ` (${sprint.uncosted} with no known cost)`}
          </span>
        </div>
        <DownloadPanel what="this sprint's records" onDownload={() => onDownload(sprint.id)} />
      </div>
    </details>
  )
}

function Current({ current, runs, onDownload }: Omit<Props, 'ended'> & { current: SprintLine }) {
  const ended = runs.filter((r) => r.result !== 'running')
  return (
    <>
      <p className={styles['purpose']}>{written(current.purpose)}</p>
      <p className={styles['small']}>
        {current.startedAt === null ? '' : `Started ${dayAndClock(current.startedAt)} · `}
        {current.done} of {current.total} issues ended
      </p>
      <h3 className={styles['subheading']}>Cost so far, by role</h3>
      {ended.length === 0 ? (
        <p className={styles['small']}>No agent has run in this sprint yet.</p>
      ) : (
        <dl className={styles['costs']}>
          {ROLES.filter((role) => ended.some((r) => r.role === role)).map((role) => (
            <RoleCost key={role} role={role} runs={ended.filter((r) => r.role === role)} />
          ))}
        </dl>
      )}
      <p className={styles['small']}>
        Costs are at API prices, from the tokens each run used, whatever plan you pay for the CLIs
        on.
      </p>
      <p className={styles['links']}>
        <Link to="/">See the issues on Now</Link>
        <Link to="/activity">See its runs in Activity</Link>
      </p>
      <DownloadPanel what="this sprint's records" onDownload={() => onDownload(current.id)} />
    </>
  )
}

/** The sprint under way and the sprints that ended. */
export function SprintsView({ current, runs, ended, onDownload }: Props) {
  return (
    <>
      <h1 className={appStyles['title']}>Sprints</h1>
      {current !== null && (
        <section className={styles['group']} aria-labelledby="this-sprint">
          <h2 className={styles['heading']} id="this-sprint">
            This sprint
          </h2>
          <Current current={current} runs={runs} onDownload={onDownload} />
        </section>
      )}
      <section className={styles['group']} aria-labelledby="ended-sprints">
        <h2 className={styles['heading']} id="ended-sprints">
          Ended sprints
        </h2>
        {ended.length === 0 ? (
          <p className={styles['muted']}>No sprint has ended yet.</p>
        ) : (
          <div className={styles['list']}>
            {ended.map((s) => (
              <Ended key={s.id} sprint={s} onDownload={onDownload} />
            ))}
          </div>
        )}
      </section>
    </>
  )
}
