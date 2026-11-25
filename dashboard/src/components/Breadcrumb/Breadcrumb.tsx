import type { ReactNode } from 'react'
import { Link } from 'react-router'
import styles from './Breadcrumb.module.css'

/** "Now / <here>": the way back from a page reached from Now. */
export function Breadcrumb({ children }: { children: ReactNode }) {
  return (
    <nav aria-label="Breadcrumb" className={styles['crumbs']}>
      <Link to="/">Now</Link> / {children}
    </nav>
  )
}
