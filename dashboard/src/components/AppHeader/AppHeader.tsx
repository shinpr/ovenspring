import { useId, useState } from 'react'
import { Link } from 'react-router'
import type { Update, VersionState } from '../../api/types.gen.ts'
import { chooseTheme, readTheme, THEME_CHOICES, type ThemeChoice } from '../../theme.ts'
import { Button } from '../Button/Button.tsx'
import { ExternalLink } from '../ExternalLink/ExternalLink.tsx'
import { controlClass } from '../Field/Field.tsx'
import { Wordmark } from '../Wordmark/Wordmark.tsx'
import styles from './AppHeader.module.css'

export type Area = 'now' | 'activity' | 'sprints' | 'settings'

const LINKS: ReadonlyArray<{ area: Area; href: string; label: string }> = [
  { area: 'now', href: '/', label: 'Now' },
  { area: 'activity', href: '/activity', label: 'Activity' },
  { area: 'sprints', href: '/sprints', label: 'Sprints' },
  { area: 'settings', href: '/settings', label: 'Settings' },
]

const THEME_NAMES: Record<ThemeChoice, string> = { auto: 'System', light: 'Light', dark: 'Dark' }

const INSTALL =
  'curl -fsSL https://github.com/shinpr/ovenspring/releases/latest/download/install.sh | sh'

/** How to restart a factory, in the folder `where` names. */
function Restart({ where }: { where: string }) {
  return (
    <p>
      Run <kbd>ovenspring stop</kbd> and then <kbd>ovenspring start</kbd> in {where}. Stopping ends
      the runs under way. They start again when the factory starts.
    </p>
  )
}

/** How to move from the running version to `update`. */
function UpdateSteps({ running, update }: { running: string; update: Update }) {
  if (update.kind === 'restart') {
    return (
      <>
        <p>
          Oven Spring {update.version} is installed, but this repository still runs {running}.
        </p>
        <Restart where="this repository's folder" />
      </>
    )
  }
  return (
    <>
      <p>
        Oven Spring {update.version} is out.{' '}
        <ExternalLink
          href={`https://github.com/shinpr/ovenspring/releases/tag/v${update.version}`}
          place="on GitHub"
        >
          What's new
        </ExternalLink>
      </p>
      <p>To update, run this in a terminal:</p>
      <pre>
        <code>{INSTALL}</code>
      </pre>
      <Restart where="the folder of each repository that runs Oven Spring" />
    </>
  )
}

interface Props {
  /** The repository the factory works on, once the daemon has reported it. */
  repository?: string | undefined
  /** The version the factory runs, and a newer one to move to, once the daemon has answered. */
  version?: VersionState | undefined
  /** The area the current page belongs to. */
  current?: Area | undefined
  /** Whether the other screens can be reached yet; before setup is saved, only the wordmark shows. */
  navigation?: boolean
}

export function AppHeader({ repository, version, current, navigation = true }: Props) {
  const [theme, setTheme] = useState<ThemeChoice>(readTheme)
  const [stepsOpen, setStepsOpen] = useState(false)
  const stepsId = useId()
  const update = version?.update ?? null
  return (
    <header className={styles['header']}>
      <div className={styles['inner']}>
        <div className={styles['brand']}>
          <Link className={styles['wordmark']} to="/" aria-label="Oven Spring">
            <Wordmark className={styles['wordmarkSvg']} />
          </Link>
          {version !== undefined && <span className={styles['version']}>v{version.running}</span>}
          {update !== null && (
            <Button
              variant="link"
              className={styles['update']}
              aria-expanded={stepsOpen}
              aria-controls={stepsId}
              onClick={() => setStepsOpen(!stepsOpen)}
            >
              {update.kind === 'restart'
                ? `Restart to run v${update.version}`
                : `v${update.version} is out`}
            </Button>
          )}
          {navigation && repository !== undefined && (
            <span className={styles['repository']}>{repository}</span>
          )}
        </div>
        {navigation && (
          <div className={styles['end']}>
            <nav className={styles['nav']} aria-label="Main">
              {LINKS.map(({ area, href, label }) => (
                <Link key={area} to={href} aria-current={area === current ? 'page' : undefined}>
                  {label}
                </Link>
              ))}
            </nav>
            <label className={styles['theme']}>
              Theme
              <select
                className={controlClass}
                value={theme}
                onChange={(e) => {
                  const choice = THEME_CHOICES.find((c) => c === e.target.value) ?? 'auto'
                  chooseTheme(choice)
                  setTheme(choice)
                }}
              >
                {THEME_CHOICES.map((c) => (
                  <option key={c} value={c}>
                    {THEME_NAMES[c]}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}
        {version !== undefined && update !== null && stepsOpen && (
          <div id={stepsId} className={styles['steps']}>
            <UpdateSteps running={version.running} update={update} />
          </div>
        )}
      </div>
    </header>
  )
}
