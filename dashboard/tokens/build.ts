// Generates `src/styles/tokens.css` from `tokens/tokens.json` (DTCG 2025.10). With `--check`, it
// writes nothing and fails when the committed CSS differs from what the tokens generate, or when a
// color pair the UI Spec's Contrast Requirements name misses its target in either theme.

import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { channels, contrastRatio } from './contrast.ts'

const here = dirname(fileURLToPath(import.meta.url))
const SOURCE = join(here, 'tokens.json')
const OUTPUT = join(here, '..', 'src', 'styles', 'tokens.css')

const THEMES = ['light', 'dark'] as const
type Theme = (typeof THEMES)[number]

/** UI Spec, Accessibility Requirements, Contrast Requirements. */
const CONTRAST: ReadonlyArray<{ foreground: string; backgrounds: string[]; target: number }> = [
  { foreground: 'text', backgrounds: ['bg', 'raised'], target: 4.5 },
  { foreground: 'text-muted', backgrounds: ['bg', 'raised'], target: 4.5 },
  { foreground: 'accent', backgrounds: ['bg', 'raised'], target: 4.5 },
  { foreground: 'ok', backgrounds: ['bg', 'raised'], target: 4.5 },
  { foreground: 'fail', backgrounds: ['bg', 'raised'], target: 4.5 },
  { foreground: 'on-accent', backgrounds: ['accent'], target: 4.5 },
  { foreground: 'field-border', backgrounds: ['bg', 'raised'], target: 3 },
]

type Group = Record<string, unknown>

function isGroup(value: unknown): value is Group {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function group(parent: Group, key: string, path: string): Group {
  const value = parent[key]
  if (!isGroup(value)) {
    throw new Error(`${path}${key} is not a group`)
  }
  return value
}

/** The tokens of a group, `$`-keys aside, by name. */
function tokens(parent: Group, path: string): Array<[string, Group]> {
  return Object.entries(parent)
    .filter(([name]) => !name.startsWith('$'))
    .map(([name, value]) => {
      if (!isGroup(value)) {
        throw new Error(`${path}${name} is not a token`)
      }
      return [name, value]
    })
}

function colorHex(token: Group, path: string): string {
  const value = token['$value']
  if (token['$type'] !== 'color' || !isGroup(value) || value['colorSpace'] !== 'srgb') {
    throw new Error(`${path} is not an sRGB color token`)
  }
  const hex = value['hex']
  const components = value['components']
  if (typeof hex !== 'string' || !Array.isArray(components)) {
    throw new Error(`${path} needs both components and hex`)
  }
  const expected = channels(hex)
  if (expected.some((c, i) => Math.abs(c - Number(components[i])) > 0.0005)) {
    throw new Error(`${path}: components do not match ${hex}`)
  }
  return hex
}

function dimension(token: Group, path: string): string {
  const value = token['$value']
  if (token['$type'] !== 'dimension' || !isGroup(value) || typeof value['value'] !== 'number') {
    throw new Error(`${path} is not a dimension token`)
  }
  if (value['unit'] !== 'px') {
    throw new Error(`${path} must be in px`)
  }
  return `${value['value']}px`
}

function fontFamily(token: Group, path: string): string {
  const value = token['$value']
  if (token['$type'] !== 'fontFamily' || !Array.isArray(value)) {
    throw new Error(`${path} is not a fontFamily token`)
  }
  const generic = new Set(['ui-sans-serif', 'ui-monospace', 'system-ui', 'sans-serif', 'monospace'])
  return value.map((f) => (generic.has(String(f)) ? String(f) : `"${String(f)}"`)).join(', ')
}

interface Parsed {
  colors: Record<Theme, Array<[string, string]>>
  common: Array<[string, string]>
}

function parse(source: unknown): Parsed {
  if (!isGroup(source)) {
    throw new Error('tokens.json is not a group')
  }
  const color = group(source, 'color', '')
  const colors = {
    light: tokens(group(color, 'light', 'color.'), 'color.light.').map(
      ([name, t]): [string, string] => [name, colorHex(t, `color.light.${name}`)]
    ),
    dark: tokens(group(color, 'dark', 'color.'), 'color.dark.').map(
      ([name, t]): [string, string] => [name, colorHex(t, `color.dark.${name}`)]
    ),
  }
  const lightNames = colors.light.map(([n]) => n).join()
  if (lightNames !== colors.dark.map(([n]) => n).join()) {
    throw new Error('color.light and color.dark must name the same roles in the same order')
  }
  const common: Array<[string, string]> = tokens(group(source, 'font', ''), 'font.').map(
    ([name, t]) => [`font-${name}`, fontFamily(t, `font.${name}`)]
  )
  for (const prefix of ['text', 'space', 'radius', 'width']) {
    for (const [name, t] of tokens(group(source, prefix, ''), `${prefix}.`)) {
      common.push([`${prefix}-${name}`, dimension(t, `${prefix}.${name}`)])
    }
  }
  return { colors, common }
}

function declarations(pairs: Array<[string, string]>, prefix: string, indent: string): string {
  return pairs.map(([name, value]) => `${indent}--${prefix}${name}: ${value};`).join('\n')
}

function css(parsed: Parsed): string {
  const light = declarations(parsed.colors.light, 'color-', '  ')
  const dark = declarations(parsed.colors.dark, 'color-', '  ')
  const darkNested = declarations(parsed.colors.dark, 'color-', '    ')
  return [
    '/* Generated by tokens/build.ts from tokens/tokens.json; edit the tokens, not this file. */',
    '',
    `:root,\n:root[data-theme="light"] {\n${light}\n}`,
    '',
    `@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]) {\n${darkNested}\n  }\n}`,
    '',
    `:root[data-theme="dark"] {\n${dark}\n}`,
    '',
    `:root {\n${declarations(parsed.common, '', '  ')}\n}`,
    '',
  ].join('\n')
}

function pairFailure(theme: Theme, palette: Map<string, string>, pair: [string, string]): string[] {
  const [foreground, background] = pair
  const fg = palette.get(foreground)
  const bg = palette.get(background)
  if (fg === undefined || bg === undefined) {
    return [`${theme}: no color ${fg === undefined ? foreground : background}`]
  }
  const target = CONTRAST.find((c) => c.foreground === foreground)?.target ?? 0
  const ratio = contrastRatio(fg, bg)
  return ratio < target
    ? [`${theme}: ${foreground} on ${background} is ${ratio.toFixed(2)}:1, below ${target}:1`]
    : []
}

function contrastFailures(parsed: Parsed): string[] {
  const pairs = CONTRAST.flatMap(({ foreground, backgrounds }) =>
    backgrounds.map((background): [string, string] => [foreground, background])
  )
  return THEMES.flatMap((theme) => {
    const palette = new Map(parsed.colors[theme])
    return pairs.flatMap((pair) => pairFailure(theme, palette, pair))
  })
}

function main(): void {
  const parsed = parse(JSON.parse(readFileSync(SOURCE, 'utf8')))
  const generated = css(parsed)
  if (!process.argv.includes('--check')) {
    writeFileSync(OUTPUT, generated)
    return
  }
  const failures = contrastFailures(parsed)
  let committed = ''
  try {
    committed = readFileSync(OUTPUT, 'utf8')
  } catch (cause) {
    throw new Error(`cannot read ${OUTPUT}`, { cause })
  }
  if (committed !== generated) {
    failures.push('src/styles/tokens.css differs from tokens/tokens.json; run `pnpm tokens`')
  }
  if (failures.length > 0) {
    process.stderr.write(`${failures.join('\n')}\n`)
    process.exitCode = 1
  }
}

main()
