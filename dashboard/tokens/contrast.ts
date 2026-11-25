// WCAG 2.2 contrast between two sRGB colors (https://www.w3.org/TR/WCAG22/#dfn-contrast-ratio).

/** The color's channels in 0..1, from its `#rrggbb` form. */
export function channels(hex: string): [number, number, number] {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex)
  if (match === null) {
    throw new Error(`not a #rrggbb color: ${hex}`)
  }
  const [, r = '', g = '', b = ''] = match
  return [Number.parseInt(r, 16) / 255, Number.parseInt(g, 16) / 255, Number.parseInt(b, 16) / 255]
}

function luminance(hex: string): number {
  const linear = channels(hex).map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
  const [r = 0, g = 0, b = 0] = linear
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

export function contrastRatio(foreground: string, background: string): number {
  const [lighter, darker] = [luminance(foreground), luminance(background)].sort((a, b) => b - a)
  return ((lighter ?? 0) + 0.05) / ((darker ?? 0) + 0.05)
}
