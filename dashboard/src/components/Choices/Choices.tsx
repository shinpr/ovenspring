import { Fragment, type ReactNode, useId } from 'react'
import styles from './Choices.module.css'

export interface Choice<T extends string> {
  value: T
  label: ReactNode
  hint?: ReactNode
  /** What the option asks for once chosen, beneath it. */
  more?: ReactNode
}

interface GroupProps {
  legend: string
  /** Read by assistive technology only, where a heading or sentence above already names it. */
  legendHidden?: boolean
  /** What follows the choices inside the group, such as a warning about the chosen one. */
  after?: ReactNode
  hint?: ReactNode
}

function Group({
  legend,
  legendHidden = false,
  after,
  hint,
  children,
}: GroupProps & { children: ReactNode }) {
  return (
    <fieldset className={styles['group']}>
      <legend className={legendHidden ? styles['hidden'] : styles['legend']}>{legend}</legend>
      {hint !== undefined && <p className={styles['hint']}>{hint}</p>}
      <div className={styles['choices']}>{children}</div>
      {after}
    </fieldset>
  )
}

/**
 * A choice's label, with its note beneath. The note describes the control instead of naming it, so
 * the choice is named by its label alone.
 */
function Label({ label, hint, id }: { label: ReactNode; hint: ReactNode; id: string }) {
  return (
    <>
      <span>{label}</span>
      {hint !== undefined && (
        <span className={styles['note']} id={id} aria-hidden="true">
          {hint}
        </span>
      )}
    </>
  )
}

interface RadioProps<T extends string> extends GroupProps {
  /** Nothing chosen yet when undefined. */
  value: T | undefined
  options: ReadonlyArray<Choice<T>>
  onChange: (value: T) => void
}

/** One choice of several, under its legend. */
export function RadioGroup<T extends string>({
  value,
  options,
  onChange,
  ...group
}: RadioProps<T>) {
  const name = useId()
  return (
    <Group {...group}>
      {options.map((o) => (
        <Fragment key={o.value}>
          <label className={styles['choice']}>
            <input
              type="radio"
              name={name}
              checked={value === o.value}
              onChange={() => onChange(o.value)}
              aria-describedby={o.hint === undefined ? undefined : `${name}-${o.value}`}
            />
            <Label label={o.label} hint={o.hint} id={`${name}-${o.value}`} />
          </label>
          {value === o.value && o.more !== undefined && (
            <div className={styles['more']}>{o.more}</div>
          )}
        </Fragment>
      ))}
    </Group>
  )
}

interface CheckProps extends GroupProps {
  values: readonly string[]
  options: ReadonlyArray<Choice<string>>
  onChange: (values: string[]) => void
}

/** Any number of choices, under their legend. */
export function CheckGroup({ values, options, onChange, ...group }: CheckProps) {
  const name = useId()
  return (
    <Group {...group}>
      {options.map((o) => (
        <label key={o.value} className={styles['choice']}>
          <input
            type="checkbox"
            checked={values.includes(o.value)}
            onChange={(e) =>
              onChange(
                e.target.checked ? [...values, o.value] : values.filter((v) => v !== o.value)
              )
            }
            aria-describedby={o.hint === undefined ? undefined : `${name}-${o.value}`}
          />
          <Label label={o.label} hint={o.hint} id={`${name}-${o.value}`} />
        </label>
      ))}
    </Group>
  )
}
