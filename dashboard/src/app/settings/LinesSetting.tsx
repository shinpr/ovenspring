import { type ReactNode, useState } from 'react'
import { Field, monoControlClass } from '../../components/Field/Field.tsx'
import { type Reasons, SettingRow } from '../../components/SettingRow/SettingRow.tsx'
import styles from './Settings.module.css'

interface Props {
  name: string
  /** What the setting changes, in one line. */
  what?: string
  /** The summary when no line is set. */
  none: string
  /** What the summary says before the lines, whatever they are. */
  before?: ReactNode
  /** The field's label in the form, where it says more than the row's name. */
  label?: string
  hint: ReactNode
  /** The server's field key the editor shows its refusal under. */
  field: string
  values: readonly string[]
  /** How the summary shows one value; as code when left out. */
  show?: (value: string) => ReactNode
  onSave: (values: string[]) => Promise<Reasons>
}

/** A setting held as one value per line. */
export function LinesSetting({
  name,
  what,
  none,
  before,
  label = name,
  hint,
  field,
  values,
  show = (value) => <code>{value}</code>,
  onSave,
}: Props) {
  const [text, setText] = useState('')
  return (
    <SettingRow
      name={name}
      {...(what === undefined ? {} : { what })}
      summary={
        <>
          {before}
          {values.length === 0 ? (
            <span className={styles['muted']}>{none}</span>
          ) : (
            values.map((v) => (
              <div key={v} className={styles['line']}>
                {show(v)}
              </div>
            ))
          )}
        </>
      }
      onOpen={() => setText(values.join('\n'))}
      onSave={() => onSave(text.split('\n').filter((l) => l.trim() !== ''))}
      fields={[field]}
    >
      {(reasons) => (
        <Field label={label} hint={hint} error={reasons[field]}>
          {(control) => (
            <textarea
              {...control}
              className={monoControlClass}
              rows={3}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          )}
        </Field>
      )}
    </SettingRow>
  )
}
