import type { ReactNode } from 'react'
import { Link } from 'react-router'
import type { ErrorNow } from '../../api/types.gen.ts'
import { settingsPath } from '../../lib/settings.ts'
import { clockOn } from '../../lib/time.ts'
import { errorFix, errorName } from '../../lib/vocabulary.ts'
import { IssueKey, Words } from './EventLine.tsx'
import styles from './Events.module.css'

interface Props {
  /** Every error going on, oldest start first; undefined while the events are not read. */
  errors: readonly ErrorNow[] | undefined
  notices: boolean
  unreadable: boolean
  now: Date
}

/** Is something wrong right now: every error started and not cleared, whatever the filters say. */
export function ErrorsNow({ errors, notices, unreadable, now }: Props) {
  const today = now.toISOString()
  let body: ReactNode = null
  if (unreadable) {
    body = <p className={styles['empty']}>Not known while the events cannot be read.</p>
  } else if (errors !== undefined && errors.length === 0) {
    body = <p className={styles['empty']}>Nothing is wrong now.</p>
  } else if (errors !== undefined) {
    body = (
      <ul className={styles['list']}>
        {errors.map((e) => (
          <li key={`${e.key}@${e.since}`} className={styles['item']}>
            <p>
              <span className={`${styles['word']} ${styles['fail']}`}>{errorName(e)}</span>{' '}
              <span className={styles['muted']}>
                since {clockOn(e.since, today)}
                {e.firstIssue !== null && (
                  <>
                    , first on <IssueKey issue={e.firstIssue} />
                  </>
                )}
                .{e.times > 1 && ` ${e.times} times, last at ${clockOn(e.last, today)}.`}
              </span>
            </p>
            <p>
              <Words parts={errorFix(e)} />
            </p>
          </li>
        ))}
      </ul>
    )
  }
  return (
    <section className={styles['group']} aria-labelledby="errors-now">
      <h2 className={styles['heading']} id="errors-now">
        Errors now
      </h2>
      {!notices && (
        <p className={styles['note']}>
          Error notices are off, so Slack tells you nothing about these.{' '}
          <Link to={settingsPath('notices')}>Turn them on in Settings</Link>
        </p>
      )}
      {body}
    </section>
  )
}
