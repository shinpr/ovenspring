import { useState } from 'react'
import { ActionBody } from '../ActionPanel/ActionPanel.tsx'
import { Button } from '../Button/Button.tsx'
import { controlClass, Field } from '../Field/Field.tsx'
import styles from './CancelPanel.module.css'

interface Props {
  /** Where the issue lives, as the owner calls it: "Linear", "GitHub". */
  tracker: string
  /** Asks the factory to cancel the issue, with the owner's reason, which may be empty. */
  onCancel: (words: string) => Promise<void>
  id?: string
  /** Closed: kept, so a request already sent still shows as sent when it opens again. */
  hidden?: boolean
}

/**
 * Ends an issue that no longer needs doing, whatever the factory is doing with it: what it does,
 * the owner's reason, and the button that does it. The page opens it from its header.
 */
export function CancelPanel({ tracker, onCancel, id, hidden = false }: Props) {
  const [words, setWords] = useState('')
  const [state, setState] = useState<'idle' | 'sending' | 'sent' | 'failed'>('idle')
  const cancel = async (): Promise<void> => {
    setState('sending')
    try {
      await onCancel(words.trim())
      setState('sent')
    } catch {
      setState('failed')
    }
  }
  return (
    <section id={id} className={styles['panel']} aria-label="Cancel this issue" hidden={hidden}>
      <ActionBody
        about={`The factory stops work on it, closes its pull request (the branch stays) and marks it canceled on ${tracker}.`}
        status={state === 'sent' ? 'Canceled. The work stops within seconds.' : ''}
        failure={state === 'failed' ? 'Canceling failed. Try again.' : null}
      >
        <Field label="Why" optional hint={`Shown on ${tracker}.`} className={styles['why']}>
          {(control) => (
            <textarea
              {...control}
              className={controlClass}
              rows={2}
              value={words}
              onChange={(e) => setWords(e.target.value)}
            />
          )}
        </Field>
        <Button
          variant="danger"
          disabled={state === 'sending' || state === 'sent'}
          onClick={() => {
            cancel().catch(() => undefined)
          }}
        >
          Cancel the issue
        </Button>
      </ActionBody>
    </section>
  )
}
