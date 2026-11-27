import { clock } from '../../lib/time.ts'
import styles from './Contact.module.css'

/** What a page shows while the factory does not answer: nothing read yet, or what it last said. */
export function Contact({
  what,
  answeredAt,
  failed,
}: {
  /** What the page reads, as the sentence's subject ("This issue"). */
  what: string
  answeredAt: Date | undefined
  failed: boolean
}) {
  if (!failed) {
    return null
  }
  if (answeredAt === undefined) {
    return (
      <p className={styles['contact']} role="alert">
        {what} could not be read: the dashboard cannot reach the factory. If it stopped, run{' '}
        <kbd>ovenspring start</kbd> in a terminal in this repository's folder.
      </p>
    )
  }
  return (
    <p className={styles['contact']} role="status">
      The dashboard lost contact with the factory. This page is as it was at{' '}
      {clock(answeredAt.toISOString())}.
    </p>
  )
}
