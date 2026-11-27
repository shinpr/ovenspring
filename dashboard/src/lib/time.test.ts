import { describe, expect, it } from 'vitest'
import { clockOn, dayHeading, duration, seconds, when } from './time.ts'

describe('duration', () => {
  const now = new Date('2026-11-21T12:00:00Z')
  it('counts whole minutes from one, then hours and the minutes left', () => {
    expect(duration('2026-11-21T11:59:50Z', now)).toBe('1 min')
    expect(duration('2026-11-21T11:54:00Z', now)).toBe('6 min')
    expect(duration('2026-11-21T11:00:00Z', now)).toBe('1 h')
    expect(duration('2026-11-21T09:55:00Z', now)).toBe('2 h 5 min')
  })
})

describe('seconds', () => {
  it('counts seconds under a minute, then minutes and the seconds left', () => {
    expect(seconds(45)).toBe('45 s')
    expect(seconds(72)).toBe('1 min 12 s')
    expect(seconds(120)).toBe('2 min')
  })
})

describe('when', () => {
  // Both moments fall on the same local day in every time zone (UTC-12 to UTC+14).
  const now = new Date('2026-11-21T06:30:00Z')
  it('gives the time alone today and adds the date on an earlier day', () => {
    expect(when('2026-11-21T06:00:00Z', now)).toMatch(/^\d{2}:\d{2}$/)
    expect(when('2026-11-19T06:00:00Z', now)).toMatch(/^Nov 1[89], \d{2}:\d{2}$/)
  })
})

describe('dayHeading', () => {
  // Noon UTC stays on the same local day from UTC-11 to UTC+11.
  const now = new Date('2026-11-27T12:00:00Z')
  it('names today and yesterday, then the date, with the year when it is another', () => {
    expect(dayHeading('2026-11-27T12:00:00Z', now)).toBe('Today, Nov 27')
    expect(dayHeading('2026-11-26T12:00:00Z', now)).toBe('Yesterday, Nov 26')
    expect(dayHeading('2026-11-25T12:00:00Z', now)).toBe('Nov 25')
    expect(dayHeading('2025-12-30T12:00:00Z', now)).toBe('Dec 30, 2025')
  })
})

describe('clockOn', () => {
  it('gives the clock on the same day and the date and clock on another', () => {
    expect(clockOn('2026-11-27T12:10:00Z', '2026-11-27T11:00:00Z')).toMatch(/^\d{2}:10$/)
    expect(clockOn('2026-11-25T12:10:00Z', '2026-11-27T11:00:00Z')).toMatch(/^Nov 25, \d{2}:10$/)
  })
})
