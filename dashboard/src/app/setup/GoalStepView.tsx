import { type FormEvent, useState } from 'react'
import { Button } from '../../components/Button/Button.tsx'
import { controlClass, Field } from '../../components/Field/Field.tsx'
import styles from './Setup.module.css'

interface Props {
  /** Saves the goal; resolves to why it was refused, or undefined when saved. */
  onStart: (text: string) => Promise<string | undefined>
}

/** Set the goal: the owner's intent, and what starts the factory. */
export function GoalStepView({ onStart }: Props) {
  const [text, setText] = useState('')
  const [error, setError] = useState<string | undefined>()
  const [saving, setSaving] = useState(false)
  const start = async (event: FormEvent): Promise<void> => {
    event.preventDefault()
    if (text.trim() === '') {
      setError('Enter the goal.')
      return
    }
    setSaving(true)
    try {
      setError(await onStart(text))
    } finally {
      setSaving(false)
    }
  }
  return (
    <>
      <div className={styles['intro']}>
        <h2 className={styles['stepTitle']}>Set the goal</h2>
        <p className={styles['text']}>
          Where the product is heading. When you start, the lead proposes the first sprint toward it
          in Slack. Nothing is worked on until you approve it there.
        </p>
      </div>
      <form
        className={styles['frame']}
        noValidate
        aria-label="Set the goal"
        onSubmit={(e) => {
          start(e).catch(() => undefined)
        }}
      >
        <Field
          label="Goal"
          hint="As the state of the product or its users, in a sentence or two, not a list of work. For example: “Shoppers find and buy the right product without having to ask support.”"
          error={error}
        >
          {(control) => (
            <textarea
              {...control}
              className={controlClass}
              rows={3}
              value={text}
              onChange={(e) => {
                setText(e.target.value)
                setError(undefined)
              }}
            />
          )}
        </Field>
        <div className={styles['actions']}>
          <Button variant="primary" type="submit" disabled={saving}>
            {saving ? 'Starting…' : 'Start'}
          </Button>
        </div>
      </form>
    </>
  )
}
