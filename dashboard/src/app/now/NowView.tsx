import { Link } from 'react-router'
import type { Overview } from '../../api/types.gen.ts'
import { FactoryStatus } from '../../components/FactoryStatus/FactoryStatus.tsx'
import {
  DecisionRow,
  DoneRow,
  GlanceSection,
  ProgressRow,
  QueuedRow,
} from '../../components/Glance/Glance.tsx'
import { dayAndClock } from '../../lib/time.ts'
import { written } from '../../lib/typed.tsx'
import styles from './Now.module.css'

interface Props {
  overview: Overview
  now: Date
  /** When the last call to the factory failed, while calls fail. */
  lostAt?: Date | undefined
  /** Opens the running step of an issue. */
  onFollow?: (key: string) => void
}

/** The factory at a glance: its state, the sprint, and every issue by who has it. */
export function NowView({ overview, now, lostAt, onFollow }: Props) {
  const { sprint, decisions, issues } = overview
  const of = (section: string) => issues.filter((i) => i.now.section === section)
  const progress = of('in-progress')
  const queued = of('waiting-to-start')
  const done = of('done')
  return (
    <>
      <FactoryStatus overview={overview} lostAt={lostAt} />
      <p className={styles['sprint']}>
        {sprint === null && overview.lastSprintEndedAt === null && (
          <span className={styles['label']}>No sprint has run yet.</span>
        )}
        {sprint === null && overview.lastSprintEndedAt !== null && (
          <>
            <span className={styles['label']}>
              No sprint running. The last sprint ended {dayAndClock(overview.lastSprintEndedAt)}.
            </span>{' '}
            <Link to="/sprints">Sprints</Link>
          </>
        )}
        {sprint !== null && (
          <>
            <span className={styles['label']}>Sprint</span> {written(sprint.purpose)}{' '}
            <span className={styles['count']}>
              · {sprint.done} of {sprint.total} ended
            </span>
          </>
        )}
      </p>
      <div className={styles['groups']}>
        <GlanceSection title="Waiting for you" count={decisions.length}>
          {decisions.map((d) => (
            <DecisionRow key={`${d.issue ?? ''}${d.ask}`} waiting={d} />
          ))}
        </GlanceSection>
        {sprint !== null && (
          <>
            <GlanceSection
              title="In progress"
              empty="Nothing is in progress."
              count={progress.length}
            >
              {progress.map((i) => (
                <ProgressRow key={i.key} issue={i} now={now} onFollow={onFollow} />
              ))}
            </GlanceSection>
            <GlanceSection title="Waiting to start" count={queued.length}>
              {queued.map((i) => (
                <QueuedRow key={i.key} issue={i} />
              ))}
            </GlanceSection>
            <GlanceSection title="Done" empty="No issue has ended yet." count={done.length}>
              {done.map((i) => (
                <DoneRow key={i.key} issue={i} now={now} />
              ))}
            </GlanceSection>
          </>
        )}
      </div>
    </>
  )
}
