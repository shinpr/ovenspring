import { useEffect, useRef } from 'react'
import { useSearchParams } from 'react-router'
import { api } from '../../api/client.ts'
import { usePoll } from '../../api/usePoll.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { SHOWS } from './EventFilters.tsx'
import { EventsView } from './EventsView.tsx'

/** The oldest line a list shows. */
function oldest(lines: ReadonlyArray<{ seq: number }>): number | undefined {
  return lines.length === 0 ? undefined : Math.min(...lines.map((l) => l.seq))
}

export function EventsPage() {
  const [params, setParams] = useSearchParams()
  const show = SHOWS.find((s) => s === params.get('show')) ?? 'all'
  const issue = params.get('issue') ?? ''
  const key = `events:${show}:${issue}`
  // The oldest line the list keeps for this filter: once set, every poll returns the lines shown
  // and whatever arrived since, so nothing shown moves out. A new filter starts from the newest.
  const from = useRef<{ key: string; seq?: number }>({ key })
  // While older lines load for this filter: the oldest line shown before them. A new filter
  // forgets it, so its first answer moves no focus.
  const loadingOlder = useRef<{ key: string; seq: number } | undefined>(undefined)
  const polled = usePoll(key, () => {
    const seq = from.current.key === key ? from.current.seq : undefined
    return api.events({
      ...(show === 'all' ? {} : { show }),
      ...(issue === '' ? {} : { issue }),
      ...(seq === undefined ? {} : { from: String(seq) }),
    })
  })
  const page = polled.data

  useEffect(() => {
    if (page === undefined) {
      return
    }
    if (from.current.key !== key || from.current.seq === undefined) {
      const seq = oldest(page.lines)
      from.current = seq === undefined ? { key } : { key, seq }
    }
    const pending = loadingOlder.current
    if (pending?.key !== key) {
      loadingOlder.current = undefined
      return
    }
    const added = page.lines.filter((l) => l.seq < pending.seq)
    if (added.length === 0) {
      return
    }
    loadingOlder.current = undefined
    // The button is gone: focus moves to the first line it brought, as the owner reads on.
    if (page.older === null) {
      document.getElementById(`event-${Math.max(...added.map((l) => l.seq))}`)?.focus()
    }
  }, [page, key])

  return (
    <>
      <Contact what="These events" answeredAt={polled.answeredAt} failed={polled.failed} />
      <EventsView
        page={page}
        unanswered={polled.failed && page === undefined}
        show={show}
        issue={issue}
        onFilter={(f) => {
          setParams(
            Object.fromEntries(
              Object.entries({ show: f.show === 'all' ? '' : f.show, issue: f.issue }).filter(
                ([, v]) => v !== ''
              )
            )
          )
        }}
        onOlder={() => {
          if (page?.older == null) {
            return
          }
          const seq = oldest(page.lines)
          loadingOlder.current = seq === undefined ? undefined : { key, seq }
          from.current = { key, seq: page.older }
          polled.refresh()
        }}
        now={new Date()}
      />
    </>
  )
}
