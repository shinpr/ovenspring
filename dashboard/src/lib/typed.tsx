import type { ReactNode } from 'react'

/** The sentence's parts, with each span between backticks drawn by `draw`. */
function spans(sentence: string, draw: (text: string, key: number) => ReactNode): ReactNode[] {
  const parts: ReactNode[] = []
  let at = 0
  sentence.split('`').forEach((part, i) => {
    parts.push(i % 2 === 1 ? draw(part, at) : part)
    at += part.length + 1
  })
  return parts
}

/** A sentence the factory wrote, with what the owner types (between backticks) shown as such. */
export function typed(sentence: string): ReactNode[] {
  return spans(sentence, (text, key) => <kbd key={key}>{text}</kbd>)
}

/** A line an agent wrote in Markdown, with its inline code (between backticks) shown as code. */
export function written(line: string): ReactNode[] {
  return spans(line, (text, key) => <code key={key}>{text}</code>)
}

/** A line an agent wrote, for a place that shows only text, such as a select's option. */
export function unmarked(line: string): string {
  return line.replaceAll('`', '')
}
