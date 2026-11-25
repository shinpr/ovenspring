import type { ComponentPropsWithRef } from 'react'
import styles from './Button.module.css'

interface Props extends ComponentPropsWithRef<'button'> {
  /**
   * `primary` for the one action a form exists for; `quiet` for a secondary action drawn as
   * accent text; `link` for an action inside text; `danger` for the button that takes an action
   * which is hard to undo, once the owner has opened it on purpose.
   */
  variant?: 'primary' | 'plain' | 'quiet' | 'link' | 'danger'
}

export function Button({ variant = 'plain', className, type = 'button', ...rest }: Props) {
  const classes = [styles[variant], className].filter(Boolean).join(' ')
  return <button type={type} className={classes} {...rest} />
}
