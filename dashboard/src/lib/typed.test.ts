import { isValidElement } from 'react'
import { describe, expect, it } from 'vitest'
import { typed, unmarked, written } from './typed.tsx'

const shape = (parts: ReturnType<typeof typed>) =>
  parts.map((p) => (isValidElement<{ children: string }>(p) ? { kbd: p.props.children } : p))

describe('typed', () => {
  it('draws what sits between backticks as something to type', () => {
    expect(shape(typed('Sign in with `gh auth login`, or enter a token in Settings.'))).toEqual([
      'Sign in with ',
      { kbd: 'gh auth login' },
      ', or enter a token in Settings.',
    ])
  })

  it('leaves a sentence without backticks as it is', () => {
    expect(shape(typed('Enter its key in Settings.'))).toEqual(['Enter its key in Settings.'])
  })
})

describe('written', () => {
  it('draws an agent line’s inline code as code', () => {
    const parts = written('Every profile in `profiles.mjs` is in the `profile` input.')
    expect(
      parts.map((p) =>
        isValidElement<{ children: string }>(p) ? { code: p.type, text: p.props.children } : p
      )
    ).toEqual([
      'Every profile in ',
      { code: 'code', text: 'profiles.mjs' },
      ' is in the ',
      { code: 'code', text: 'profile' },
      ' input.',
    ])
  })
})

describe('unmarked', () => {
  it('leaves the words of inline code without its backticks', () => {
    expect(unmarked('List each `profile` value')).toBe('List each profile value')
  })
})
