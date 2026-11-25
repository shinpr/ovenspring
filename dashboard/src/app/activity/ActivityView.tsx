import type { LaunchRow } from '../../api/types.gen.ts'
import { RunLine } from '../../components/RunLine/RunLine.tsx'
import { clock } from '../../lib/time.ts'
import appStyles from '../App.module.css'
import styles from './Activity.module.css'

export interface Option {
  value: string
  label: string
}

interface Props {
  /** The runs the filter selects, newest first; undefined while they are read. */
  runs: readonly LaunchRow[] | undefined
  sprints: readonly Option[]
  issues: readonly Option[]
  sprint: string
  issue: string
  onFilter: (filter: { sprint: string; issue: string }) => void
  onOpenRun: (id: string) => void
  now: Date
}

/** The hour a run ended in, as its heading: "14:00". */
function hour(run: LaunchRow): string {
  return `${clock(run.endedAt ?? run.startedAt).slice(0, 2)}:00`
}

function byHour(runs: readonly LaunchRow[]): Array<[string, LaunchRow[]]> {
  const groups: Array<[string, LaunchRow[]]> = []
  for (const run of runs) {
    const h = hour(run)
    const last = groups.at(-1)
    if (last?.[0] === h) {
      last[1].push(run)
    } else {
      groups.push([h, [run]])
    }
  }
  return groups
}

/** Every agent run, newest first: what is under way, then the earlier ones by hour. */
export function ActivityView(props: Props) {
  const { runs, sprints, issues, sprint, issue, onFilter, onOpenRun, now } = props
  const under = (runs ?? []).filter((r) => r.result === 'running')
  const earlier = (runs ?? []).filter((r) => r.result !== 'running')
  const line = (r: LaunchRow) => (
    <RunLine key={r.id} run={r} now={now} onOpen={onOpenRun} place="activity" />
  )
  return (
    <>
      <h1 className={appStyles['title']}>Activity</h1>
      <p className={styles['intro']}>Every agent run in this sprint, newest first.</p>
      <div className={styles['filters']}>
        <label>
          Sprint{' '}
          <select
            className={styles['select']}
            value={sprint}
            onChange={(e) => onFilter({ sprint: e.target.value, issue: '' })}
          >
            {sprints.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Issue{' '}
          <select
            className={styles['select']}
            value={issue}
            onChange={(e) => onFilter({ sprint, issue: e.target.value })}
          >
            <option value="">All issues</option>
            {issues.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <section className={styles['group']} aria-labelledby="under-way">
        <h2 className={styles['heading']} id="under-way">
          Under way
        </h2>
        <ul className={styles['runs']}>
          {/* Empty only once answered: while reading, or when it could not be read, nothing. */}
          {runs !== undefined && under.length === 0 ? (
            <li className={styles['empty']}>Nothing is running for this filter.</li>
          ) : (
            under.map(line)
          )}
        </ul>
      </section>
      <section className={styles['group']} aria-labelledby="earlier">
        <h2 className={styles['heading']} id="earlier">
          Earlier
        </h2>
        {runs !== undefined && earlier.length === 0 && (
          <p className={styles['empty']}>Nothing has happened yet.</p>
        )}
        {byHour(earlier).map(([h, group]) => (
          <section key={`${h}-${group[0]?.id ?? ''}`} aria-label={h}>
            <h3 className={styles['hour']}>{h}</h3>
            <ul className={styles['runs']}>{group.map(line)}</ul>
          </section>
        ))}
      </section>
    </>
  )
}
