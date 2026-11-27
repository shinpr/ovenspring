import type { IssueTitle } from '../../api/types.gen.ts'
import styles from './Events.module.css'

export const SHOWS = ['all', 'errors', 'filter'] as const
type Show = (typeof SHOWS)[number]

const SHOW_WORDS: Record<Show, string> = {
  all: 'Everything',
  errors: 'Errors and when they cleared',
  filter: "The issue filter's drops and failures",
}

export interface Filter {
  show: Show
  /** An issue key, or empty for every issue. */
  issue: string
}

interface Props extends Filter {
  /** Every issue a recorded line concerns, by key. */
  issues: readonly IssueTitle[]
  onFilter: (filter: Filter) => void
}

/** Show and Issue: what the list keeps. */
export function EventFilters({ show, issue, issues, onFilter }: Props) {
  const options =
    issue === '' || issues.some((i) => i.key === issue)
      ? issues
      : [...issues, { key: issue, title: null }]
  return (
    <div className={styles['filters']}>
      <label>
        Show{' '}
        <select
          className={styles['select']}
          value={show}
          onChange={(e) =>
            onFilter({ show: SHOWS.find((s) => s === e.target.value) ?? 'all', issue })
          }
        >
          {SHOWS.map((s) => (
            <option key={s} value={s}>
              {SHOW_WORDS[s]}
            </option>
          ))}
        </select>
      </label>
      <label>
        Issue{' '}
        <select
          className={styles['select']}
          value={issue}
          onChange={(e) => onFilter({ show, issue: e.target.value })}
        >
          <option value="">All issues</option>
          {options.map((i) => (
            <option key={i.key} value={i.key}>
              {i.title === null ? i.key : `${i.key} ${i.title}`}
            </option>
          ))}
        </select>
      </label>
    </div>
  )
}
