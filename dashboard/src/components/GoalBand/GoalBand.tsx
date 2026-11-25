import { Link } from 'react-router'
import styles from './GoalBand.module.css'

/** The product goal on every screen: the factory works toward it in everything it does. Setup's
 * last step sets it, so there always is one here. */
export function GoalBand({ goal }: { goal: string }) {
  return (
    <section className={styles['band']} aria-label="Product goal">
      <div className={styles['inner']}>
        <span className={styles['label']}>Goal</span>
        <p className={styles['text']}>{goal}</p>
        <Link className={styles['link']} to="/goal">
          Rewrite…
        </Link>
      </div>
    </section>
  )
}
