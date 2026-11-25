import type { ReactNode } from 'react'
import styles from './Setup.module.css'

const STEPS = ['Connect', 'Run the product', 'Set the goal'] as const

/** Setup's title and its three steps, the current one marked, around the step's own page. */
export function SetupFrame({
  repository,
  step,
  children,
}: {
  repository: string
  /** 1, 2 or 3. */
  step: number
  children: ReactNode
}) {
  return (
    <div className={styles['frame']}>
      <div>
        <h1 className={styles['title']}>Set up Oven Spring for {repository}</h1>
        <ol className={styles['steps']} aria-label="Setup steps">
          {STEPS.map((name, i) => (
            <li key={name} {...(step === i + 1 ? { 'aria-current': 'step' as const } : {})}>
              <span className={styles['number']}>{i + 1}</span>
              {name}
              {step > i + 1 && (
                <>
                  {' '}
                  <span className={styles['ok']} aria-hidden="true">
                    ✓
                  </span>
                  <span className={styles['hidden']}> (done)</span>
                </>
              )}
            </li>
          ))}
        </ol>
      </div>
      {children}
    </div>
  )
}
