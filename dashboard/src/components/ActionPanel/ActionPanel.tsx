import type { ReactNode } from 'react'
import styles from './ActionPanel.module.css'

interface BodyProps {
  /** What the action does, read before taking it. */
  about: ReactNode
  /** The fields and the button that take the action. */
  children: ReactNode
  /** What the action is doing or did, beside the button. */
  status: string
  /** Why the action failed, when it did. */
  failure: string | null
}

/** What an action does, the fields and button that take it, and how it went. */
export function ActionBody({ about, children, status, failure }: BodyProps) {
  return (
    <>
      <p className={styles['about']}>{about}</p>
      <div className={styles['actions']}>
        {children}
        <span className={styles['state']} role="status">
          {status}
        </span>
      </div>
      {failure !== null && (
        <p className={styles['failed']} role="alert">
          {failure}
        </p>
      )}
    </>
  )
}

interface Props extends BodyProps {
  /** The action, as the line that opens the panel says it. */
  summary: string
  open?: boolean
}

/** An action the owner takes now and then, collapsed until asked for. */
export function ActionPanel({ summary, open = false, ...body }: Props) {
  return (
    <details className={styles['panel']} open={open}>
      <summary className={styles['toggle']}>{summary}</summary>
      <ActionBody {...body} />
    </details>
  )
}
