import type { ReactNode } from 'react'
import styles from './Warning.module.css'

interface Props {
  /** "Warning" for a risk the owner accepts by choosing; "Note" for what a choice changed. */
  kind?: 'Warning' | 'Note'
  children: ReactNode
  id?: string
}

/** A line under a choice that says what choosing it gives up. */
export function Warning({ kind = 'Warning', children, id }: Props) {
  return (
    <p className={styles['warning']} id={id}>
      <b>{kind}:</b> {children}
    </p>
  )
}
