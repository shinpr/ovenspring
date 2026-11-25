import { type ReactNode, type Ref, useId, useState } from 'react'
import styles from './Field.module.css'

interface Props {
  label: string
  /** Says "(optional)" after the label, in the muted colour. */
  optional?: boolean
  hint?: ReactNode
  /** Why the value was refused, shown under the field. */
  error?: string | undefined
  /** Renders the control with the ids that tie it to its label, hint and error. */
  children: (control: {
    id: string
    'aria-describedby'?: string
    'aria-invalid'?: true
  }) => ReactNode
  className?: string | undefined
}

/** A labelled control with its hint and the reason it was refused. */
export function Field({ label, optional = false, hint, error, children, className }: Props) {
  const id = useId()
  const described = [
    hint === undefined ? '' : `${id}-hint`,
    error === undefined ? '' : `${id}-error`,
  ]
    .filter(Boolean)
    .join(' ')
  return (
    <div className={[styles['field'], className].filter(Boolean).join(' ')}>
      <label className={styles['label']} htmlFor={id}>
        {label}
        {optional && <span className={styles['optional']}> (optional)</span>}
      </label>
      {hint !== undefined && (
        <span className={styles['hint']} id={`${id}-hint`}>
          {hint}
        </span>
      )}
      {children({
        id,
        ...(described === '' ? {} : { 'aria-describedby': described }),
        ...(error === undefined ? {} : { 'aria-invalid': true }),
      })}
      {error !== undefined && (
        <span className={styles['error']} id={`${id}-error`}>
          {error}
        </span>
      )}
    </div>
  )
}

export const controlClass = styles['control'] ?? ''
export const monoControlClass = `${styles['control'] ?? ''} ${styles['mono'] ?? ''}`

interface SecretProps {
  /** What the value is, for the Show and Hide button's name. */
  name: string
  value: string
  onChange: (value: string) => void
  onBlur?: () => void
  control: Parameters<Props['children']>[0]
  ref?: Ref<HTMLInputElement>
}

/** A token or key: hidden as it is typed, with a button that shows it to check what was pasted. */
export function SecretInput({ name, value, onChange, onBlur, control, ref }: SecretProps) {
  const [shown, setShown] = useState(false)
  return (
    <span className={styles['secret']}>
      <input
        {...control}
        ref={ref}
        className={controlClass}
        type={shown ? 'text' : 'password'}
        autoComplete="off"
        spellCheck={false}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
      />
      <button
        type="button"
        className={styles['reveal']}
        aria-controls={control.id}
        aria-pressed={shown}
        aria-label={`${shown ? 'Hide' : 'Show'} ${name}`}
        onClick={() => setShown((s) => !s)}
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z" />
          <circle cx="8" cy="8" r="2" />
          {shown && <path d="M2.5 13.5l11-11" />}
        </svg>
      </button>
    </span>
  )
}
