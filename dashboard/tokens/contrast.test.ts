import { describe, expect, it } from 'vitest'
import { contrastRatio } from './contrast.ts'

describe('contrastRatio', () => {
  it('gives the extremes and a known mid value of the WCAG formula', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 5)
    expect(contrastRatio('#ffffff', '#ffffff')).toBeCloseTo(1, 5)
    // #767676 on white is the commonly cited lightest grey that reaches 4.5:1.
    expect(contrastRatio('#767676', '#ffffff')).toBeCloseTo(4.54, 2)
  })

  it('does not depend on which color is the foreground', () => {
    expect(contrastRatio('#9c4416', '#f7f3ec')).toBe(contrastRatio('#f7f3ec', '#9c4416'))
  })

  it('refuses a color that is not #rrggbb', () => {
    expect(() => contrastRatio('red', '#ffffff')).toThrow('not a #rrggbb color: red')
  })
})
