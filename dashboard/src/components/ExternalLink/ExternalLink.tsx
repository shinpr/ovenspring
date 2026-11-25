import type { ReactNode } from 'react'
import styles from './ExternalLink.module.css'

/** A link that opens outside the dashboard, in a new tab, and says so. */
export function ExternalLink({
  href,
  className,
  place,
  children,
}: {
  href: string
  className?: string | undefined
  /** Where it opens, for a screen reader ("in Slack"). */
  place?: string | undefined
  children: ReactNode
}) {
  return (
    <a href={href} target="_blank" rel="noopener" className={className}>
      {children}
      <span aria-hidden="true"> ↗</span>
      <span className={styles['sr']}>
        {place === undefined ? '' : ` ${place}`} (opens in a new tab)
      </span>
    </a>
  )
}
