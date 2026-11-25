import { type ReactNode, useEffect, useRef, useState } from 'react'
import { Button } from '../../components/Button/Button.tsx'
import { Field, SecretInput } from '../../components/Field/Field.tsx'
import styles from './Settings.module.css'

interface Props {
  label: string
  /** What to enter and where to get it, above what is saved. */
  help?: ReactNode
  /** The saved one's last four characters; undefined when none is saved. */
  saved: string | undefined
  /** Empty while the saved one is kept. */
  value: string
  onChange: (value: string) => void
  error?: string | undefined
}

function withHelp(help: ReactNode, state: string): ReactNode {
  return help === undefined ? (
    state
  ) : (
    <>
      {help}
      <br />
      {state}
    </>
  )
}

/**
 * A token or key. A saved one is named by how it ends and kept until the owner chooses to replace
 * it, so it is never overwritten by a stray keystroke.
 */
export function SecretSetting({ label, help, saved, value, onChange, error }: Props) {
  const [replacing, setReplacing] = useState(false)
  const input = useRef<HTMLInputElement>(null)
  const replace = useRef<HTMLButtonElement>(null)
  // Focus follows the owner's own switch, not the form opening.
  const switched = useRef<boolean>(false)
  useEffect(() => {
    if (switched.current) {
      ;(replacing ? input : replace).current?.focus()
    }
  }, [replacing])
  const switchTo = (next: boolean): void => {
    switched.current = true
    setReplacing(next)
  }

  if (saved !== undefined && !replacing) {
    return (
      <Field label={label} hint={withHelp(help, `Saved, ending in ${saved}.`)} error={error}>
        {(control) => (
          <Button
            {...control}
            ref={replace}
            aria-label={`Replace ${label}`}
            className={styles['secretAction']}
            onClick={() => switchTo(true)}
          >
            Replace
          </Button>
        )}
      </Field>
    )
  }
  return (
    <Field
      label={label}
      hint={withHelp(
        help,
        saved === undefined ? 'Not saved yet.' : `Replaces the saved one, ending in ${saved}.`
      )}
      error={error}
    >
      {(control) => (
        <>
          <SecretInput
            ref={input}
            name={label}
            value={value}
            onChange={onChange}
            control={control}
          />
          {saved !== undefined && (
            <Button
              variant="link"
              className={styles['keep']}
              onClick={() => {
                onChange('')
                switchTo(false)
              }}
            >
              Keep the saved one
            </Button>
          )}
        </>
      )}
    </Field>
  )
}
