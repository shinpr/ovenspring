import type { EventsPage, EventLine as Line } from '../../api/types.gen.ts'
import { Button } from '../../components/Button/Button.tsx'
import { dayHeading, localDay } from '../../lib/time.ts'
import appStyles from '../App.module.css'
import { ErrorsNow } from './ErrorsNow.tsx'
import { EventFilters, type Filter } from './EventFilters.tsx'
import { EventLine } from './EventLine.tsx'
import styles from './Events.module.css'

interface Props extends Filter {
  /** The last answer; undefined while the events are read. */
  page: EventsPage | undefined
  /** Nothing could be read yet: the page shows its title alone. */
  unanswered: boolean
  onFilter: (filter: Filter) => void
  onOlder: () => void
  now: Date
}

/** Lines in their order, newest first, grouped by the local day they fall on. */
function byDay(lines: readonly Line[]): Array<[string, Line[]]> {
  const groups: Array<[string, Line[]]> = []
  for (const line of lines) {
    const day = localDay(line.at)
    const last = groups.at(-1)
    if (last?.[0] === day) {
      last[1].push(line)
    } else {
      groups.push([day, [line]])
    }
  }
  return groups
}

function List({
  page,
  filtered,
  now,
  onOlder,
}: {
  page: EventsPage
  filtered: boolean
  now: Date
  onOlder: () => void
}) {
  if (page.unreadable) {
    return (
      <p className={styles['empty']}>
        The events could not be read. The dashboard tries again in a few seconds.
      </p>
    )
  }
  if (!page.recorded) {
    return <p className={styles['empty']}>Nothing has been recorded yet.</p>
  }
  if (page.lines.length === 0) {
    return <p className={styles['empty']}>No event matches this filter.</p>
  }
  return (
    <>
      {byDay(page.lines).map(([day, lines]) => (
        <section key={day} aria-labelledby={`day-${day}`}>
          <h3 className={styles['day']} id={`day-${day}`}>
            {dayHeading(lines[0]?.at ?? day, now)}
          </h3>
          <ul className={styles['list']}>
            {lines.map((line) => (
              <EventLine key={line.seq} line={line} />
            ))}
          </ul>
        </section>
      ))}
      {page.older !== null ? (
        <div className={styles['more']}>
          <Button onClick={onOlder}>Show older events</Button>
        </div>
      ) : (
        !filtered && <p className={styles['empty']}>That is everything the factory has recorded.</p>
      )}
    </>
  )
}

/** S-08: what the factory did and whether something is wrong now. */
export function EventsView(props: Props) {
  const { page, unanswered, show, issue, onFilter, onOlder, now } = props
  return (
    <>
      <h1 className={appStyles['title']}>Events</h1>
      <p className={styles['intro']}>Everything the factory recorded, newest first.</p>
      {!unanswered && (
        <>
          <ErrorsNow
            errors={page?.errorsNow}
            notices={page?.notices ?? true}
            unreadable={page?.unreadable ?? false}
            now={now}
          />
          <section className={styles['group']} aria-labelledby="what-the-factory-did">
            <h2 className={styles['heading']} id="what-the-factory-did">
              What the factory did
            </h2>
            <EventFilters
              show={show}
              issue={issue}
              issues={page?.issues ?? []}
              onFilter={onFilter}
            />
            <div className={styles['days']}>
              {page !== undefined && (
                <List
                  page={page}
                  filtered={show !== 'all' || issue !== ''}
                  now={now}
                  onOlder={onOlder}
                />
              )}
            </div>
          </section>
        </>
      )}
    </>
  )
}
