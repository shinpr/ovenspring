/** How long since `iso`, short: "3 min", "2 h 5 min". */
export function duration(iso: string, now: Date): string {
  return span(new Date(iso), now)
}

/** The time between two moments, short: "3 min", "2 h 5 min". */
export function span(from: Date, to: Date): string {
  const minutes = Math.max(1, Math.floor((to.getTime() - from.getTime()) / 60_000))
  if (minutes < 60) {
    return `${minutes} min`
  }
  const rest = minutes % 60
  return rest === 0 ? `${Math.floor(minutes / 60)} h` : `${Math.floor(minutes / 60)} h ${rest} min`
}

/** A short wait to the second: "45 s", "1 min 12 s", "2 min". */
export function seconds(total: number): string {
  const minutes = Math.floor(total / 60)
  const rest = Math.round(total % 60)
  if (minutes === 0) {
    return `${rest} s`
  }
  return rest === 0 ? `${minutes} min` : `${minutes} min ${rest} s`
}

/** The local time of day, as the owner reads it on their clock: "14:10". */
export function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
}

/** The local date, short: "Nov 21". */
function day(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', { day: 'numeric', month: 'short' })
}

/** The days a sprint ran: "Nov 21 – Nov 22", or from its start while it has not ended. */
export function days(from: string | null, to: string | null): string {
  if (from === null) {
    return ''
  }
  if (to === null) {
    return `From ${day(from)}`
  }
  return day(from) === day(to) ? day(from) : `${day(from)} – ${day(to)}`
}

/** The local date and time: "Nov 23, 10:02". */
export function dayAndClock(iso: string): string {
  return `${day(iso)}, ${clock(iso)}`
}

/** When something happened: the time alone today, with its date on an earlier day. */
export function when(iso: string, now: Date): string {
  return day(iso) === day(now.toISOString()) ? clock(iso) : dayAndClock(iso)
}
