import { type FormEvent, useState } from 'react'
import { Button } from '../../components/Button/Button.tsx'
import { controlClass } from '../../components/Field/Field.tsx'
import appStyles from '../App.module.css'
import styles from './Goal.module.css'

interface Props {
  goal: string
  /** Stores the new goal; resolves to why it was refused, or undefined when saved. */
  onSave: (text: string) => Promise<string | undefined>
  onCancel: () => void
}

/** Rewriting the product goal, with what changes when it is saved. */
export function GoalView({ goal, onSave, onCancel }: Props) {
  const [draft, setDraft] = useState(goal)
  const [error, setError] = useState<string | undefined>()
  const [saving, setSaving] = useState(false)
  const changed = draft.trim() !== '' && draft.trim() !== goal.trim()
  const save = async (event: FormEvent): Promise<void> => {
    event.preventDefault()
    setSaving(true)
    try {
      setError(await onSave(draft))
    } finally {
      setSaving(false)
    }
  }
  return (
    <>
      <h1 className={appStyles['title']}>Rewrite the product goal</h1>
      <section className={styles['group']} aria-labelledby="goal-changes">
        <h2 className={styles['heading']} id="goal-changes">
          What changes when you save
        </h2>
        <ul className={styles['list']}>
          <li>Agent runs from then on get the new goal, in the sprint under way too.</li>
          <li>The sprint under way keeps its purpose and issues.</li>
          <li>The next sprint is proposed toward the new goal.</li>
        </ul>
      </section>
      <form
        className={styles['group']}
        aria-labelledby="goal-new"
        onSubmit={(e) => {
          save(e).catch(() => undefined)
        }}
      >
        <h2 className={styles['heading']} id="goal-new">
          New goal
        </h2>
        <p className={styles['help']} id="goal-help">
          Where the product is heading, as the state of the product or its users, not the work to
          do.
        </p>
        <textarea
          className={`${controlClass} ${styles['text']}`}
          aria-labelledby="goal-new"
          aria-describedby={error === undefined ? 'goal-help' : 'goal-help goal-error'}
          aria-invalid={error === undefined ? undefined : true}
          rows={3}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        {error !== undefined && (
          <p className={styles['error']} id="goal-error">
            {error}
          </p>
        )}
        <div className={styles['actions']}>
          <Button variant="primary" type="submit" disabled={!changed || saving}>
            {saving ? 'Saving…' : 'Save the new goal'}
          </Button>
          <Button onClick={onCancel}>Cancel</Button>
          {!changed && (
            <span className={styles['muted']}>
              {draft.trim() === '' ? 'A goal cannot be empty.' : 'Change the text to save.'}
            </span>
          )}
        </div>
      </form>
    </>
  )
}
