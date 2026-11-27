import {
  createContext,
  type FormEvent,
  type ReactNode,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react'
import { Button } from '../Button/Button.tsx'
import styles from './SettingRow.module.css'

/** Why a save was refused, by the server's field key, or by section or `request`. */
export type Reasons = Readonly<Record<string, string>>

interface Editing {
  /** The row whose form is open. */
  current: string | undefined
  set: (row: string | undefined) => void
  /** The open form was typed in, so leaving it discards a change. */
  changed?: () => void
}

const EditingRow = createContext<Editing | null>(null)

/** Rows under one provider open one at a time: opening one closes the other. */
export const EditingProvider = EditingRow.Provider

interface Props {
  name: string
  /** What the setting changes, in one line. */
  what?: string
  summary: ReactNode
  /** Shows the summary in the mono face, for identifiers and commands. */
  mono?: boolean
  /** `block`: the name and Change on one line, what it changes and the summary beneath. */
  layout?: 'row' | 'block'
  initiallyOpen?: boolean
  /** Resets the editor's draft from the saved values, as the editor opens. */
  onOpen?: () => void
  /** Saves the draft; resolves to the reasons it was refused, empty when it was saved. */
  onSave?: () => Promise<Reasons>
  /** The reasons the editor shows under its own fields; the rest show under the buttons. */
  fields?: readonly string[]
  /** The save checks the value against its service, or starts the product, before it is kept. */
  checks?: boolean
  /** When a saved change takes effect, under the buttons. */
  applies?: string
  /** The editor's fields, given the reasons the last save was refused; none for a fixed row. */
  children?: (reasons: Reasons) => ReactNode
}

function useOpen(name: string, initiallyOpen: boolean): Editing {
  const shared = useContext(EditingRow)
  const [local, setLocal] = useState<string | undefined>(initiallyOpen ? name : undefined)
  return shared ?? { current: local, set: setLocal }
}

/** Focuses `el`, scrolling only as far as it needs to be in view. */
function reach(el: HTMLElement | null | undefined): void {
  el?.focus({ preventScroll: true })
  el?.scrollIntoView({ block: 'nearest' })
}

/** The form's first control; in a group of radios, the one chosen, as the arrow keys move from it. */
function firstControl(form: HTMLFormElement | null): HTMLElement | null {
  const first = form?.querySelector<HTMLElement>('input, select, textarea') ?? null
  if (!(first instanceof HTMLInputElement) || first.type !== 'radio') {
    return first
  }
  const group = form?.querySelectorAll<HTMLInputElement>('input[type="radio"]') ?? []
  return Array.from(group).find((r) => r.name === first.name && r.checked) ?? first
}

/** One setting: what it changes and its whole value, and a form that opens under it. */
export function SettingRow({
  name,
  what,
  summary,
  mono = false,
  layout = 'row',
  initiallyOpen = false,
  onOpen,
  onSave,
  fields = [],
  checks = false,
  applies = 'Applies from the next run.',
  children,
}: Props) {
  const editing = useOpen(name, initiallyOpen)
  const open = editing.current === name
  const [saving, setSaving] = useState(false)
  const [reasons, setReasons] = useState<Reasons>({})
  const item = useRef<HTMLLIElement>(null)
  const change = useRef<HTMLButtonElement>(null)
  const form = useRef<HTMLFormElement>(null)
  const moved = useRef<boolean>(false)
  // Set by this row's own Cancel or save: where the row ended on screen when the owner was
  // inside its form, or null when its top was in view.
  const closing = useRef<{ bottom: number | null } | null>(null)
  useEffect(() => {
    // Focus follows the owner's own opening and closing, not the first render. Moving it never
    // scrolls more than the focused control needs to be seen.
    if (!moved.current) {
      moved.current = true
      return
    }
    if (open) {
      reach(firstControl(form.current))
      return
    }
    const closed = closing.current
    closing.current = null
    if (closed === null) {
      return
    }
    // Scrolled into the form, the owner keeps what follows the row where it was; the collapsed
    // row ends just above it.
    if (closed.bottom !== null && item.current !== null) {
      window.scrollBy(0, item.current.getBoundingClientRect().bottom - closed.bottom)
    }
    reach(change.current)
  }, [open])
  const close = (): void => {
    const box = item.current?.getBoundingClientRect()
    closing.current = { bottom: box === undefined || box.top >= 0 ? null : box.bottom }
    setReasons({})
    editing.set(undefined)
  }
  const save = async (event: FormEvent): Promise<void> => {
    event.preventDefault()
    setSaving(true)
    try {
      const refused = (await onSave?.()) ?? {}
      setReasons(refused)
      if (Object.keys(refused).length === 0) {
        close()
      }
    } catch {
      setReasons({ request: 'It could not be saved. Try again.' })
    } finally {
      setSaving(false)
    }
  }
  // A refusal under a field takes the focus there, so it is heard as well as seen.
  useEffect(() => {
    if (Object.keys(reasons).some((key) => fields.includes(key))) {
      form.current?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus()
    }
  }, [reasons, fields])
  const others = Object.entries(reasons).filter(([key]) => !fields.includes(key))
  const [idle, busy] = checks ? ['Check and save', 'Checking…'] : ['Save', 'Saving…']
  // Kept in place while the form is open, so the row's summary keeps its width and line breaks.
  const changeButton = children !== undefined && (
    <Button
      ref={change}
      className={open ? styles['concealed'] : undefined}
      aria-label={`Change ${name}`}
      onClick={() => {
        setReasons({})
        onOpen?.()
        editing.set(name)
      }}
    >
      Change
    </Button>
  )
  const summaryClass = mono ? `${styles['summary']} ${styles['mono']}` : styles['summary']
  return (
    <li ref={item} className={styles['item']}>
      {layout === 'block' ? (
        <>
          <div className={styles['blockHead']}>
            <div className={styles['blockName']}>{name}</div>
            {changeButton}
          </div>
          {what !== undefined && <div className={styles['what']}>{what}</div>}
          <div className={`${summaryClass} ${styles['blockSummary']}`}>{summary}</div>
        </>
      ) : (
        <div className={styles['row']}>
          <div>
            <div className={styles['name']}>{name}</div>
            {what !== undefined && <div className={styles['what']}>{what}</div>}
          </div>
          <div className={summaryClass}>{summary}</div>
          <div>{changeButton}</div>
        </div>
      )}
      {open && children !== undefined && (
        <form
          className={styles['editor']}
          ref={form}
          aria-label={name}
          onInput={() => editing.changed?.()}
          onChange={() => editing.changed?.()}
          onSubmit={(e) => {
            save(e).catch(() => undefined)
          }}
        >
          <div className={styles['fields']}>{children(reasons)}</div>
          <div className={styles['actions']}>
            <Button variant="primary" type="submit" disabled={saving}>
              {saving ? busy : idle}
            </Button>
            <Button onClick={close}>Cancel</Button>
            <span className={styles['applies']}>{applies}</span>
          </div>
          {others.map(([key, reason]) => (
            <p key={key} className={styles['refused']} role="alert">
              {reason}
            </p>
          ))}
        </form>
      )}
    </li>
  )
}
