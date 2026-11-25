import { useCallback, useEffect, useRef, useState } from 'react'

/** How often a view asks again while it is visible. */
const POLL_MS = 5000

export interface Polled<T> {
  /** The last answer, kept while later calls fail. */
  data: T | undefined
  answeredAt: Date | undefined
  /** The latest call failed: what is shown is from `answeredAt`, or nothing could be read. */
  failed: boolean
  /** Asks again now, as after a save. */
  refresh: () => void
}

/** What the calls for one `key` returned; a page that asks for something else starts empty. */
interface Answered<T> {
  key: string
  data?: T
  answeredAt?: Date
  failed: boolean
}

/**
 * Calls `load` now and every five seconds while the page is visible, keeping the last answer.
 * `key` names what is read (an issue, a filter): when it changes, `load` is called at once and
 * nothing of the previous one is shown. `load` itself may be a new function on every render.
 */
export function usePoll<T>(key: string, load: () => Promise<T>): Polled<T> {
  const [answered, setAnswered] = useState<Answered<T>>({ key, failed: false })
  const loadRef = useRef(load)
  loadRef.current = load
  // Only the latest call's answer is kept: an earlier call can answer after a later one, and a
  // call can answer after the page has gone.
  const latest = useRef(0)

  const refresh = useCallback((): void => {
    latest.current += 1
    const call = latest.current
    loadRef.current().then(
      (data) => {
        if (call === latest.current) {
          setAnswered({ key, data, answeredAt: new Date(), failed: false })
        }
      },
      () => {
        if (call === latest.current) {
          setAnswered((a) => ({ ...(a.key === key ? a : { key }), failed: true }))
        }
      }
    )
  }, [key])

  useEffect(() => {
    refresh()
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') {
        refresh()
      }
    }, POLL_MS)
    const onVisible = (): void => {
      if (document.visibilityState === 'visible') {
        refresh()
      }
    }
    document.addEventListener('visibilitychange', onVisible)
    return (): void => {
      latest.current += 1
      window.clearInterval(timer)
      document.removeEventListener('visibilitychange', onVisible)
    }
  }, [refresh])

  const current = answered.key === key ? answered : undefined
  return {
    data: current?.data,
    answeredAt: current?.answeredAt,
    failed: current?.failed ?? false,
    refresh,
  }
}
