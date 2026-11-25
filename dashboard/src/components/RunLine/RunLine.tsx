import { Link } from 'react-router'
import type { LaunchRow } from '../../api/types.gen.ts'
import { clock, duration } from '../../lib/time.ts'
import { resultTone, runSentence } from '../../lib/vocabulary.ts'
import styles from './RunLine.module.css'

interface Props {
  run: LaunchRow
  now: Date
  /** Opens the run panel. */
  onOpen: (id: string) => void
  /**
   * `activity`: when it ended (or started, while it runs) and the issue it belongs to.
   * `path`: an issue's own run, with when it started and ended.
   */
  place: 'activity' | 'path'
}

/** Activity names when a run ended; the path, when it started and ended; both, the start of one under way. */
function when(run: LaunchRow, place: Props['place']): string {
  const started = clock(run.startedAt)
  if (run.endedAt === null) {
    return `${started}–`
  }
  return place === 'activity' ? clock(run.endedAt) : `${started}–${clock(run.endedAt)}`
}

/** One agent run as a line: when, who did what, how it ended, and the way into it. */
export function RunLine({ run, now, onOpen, place }: Props) {
  const running = run.result === 'running'
  const { who, what, result } = runSentence(run)
  const tone = resultTone(run.result)
  return (
    <li className={styles[place]}>
      <span className={styles['time']}>{when(run, place)}</span>
      {place === 'activity' && (
        <span className={styles['issue']}>
          {run.issue !== null && <Link to={`/issues/${run.issue}`}>{run.issue}</Link>}
        </span>
      )}
      <span className={styles['text']}>
        <span className={styles['who']}>{who}</span> {what}
        {running && <span className={styles['muted']}> · for {duration(run.startedAt, now)}</span>}
        {result !== undefined && <span className={styles[tone]}> → {result}</span>}
      </span>
      <button type="button" className={styles['open']} onClick={() => onOpen(run.id)}>
        {running ? 'follow' : 'open run'}
      </button>
    </li>
  )
}
