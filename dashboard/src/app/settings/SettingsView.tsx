import { type ReactNode, useEffect, useRef, useState } from 'react'
import { Link } from 'react-router'
import type { Settings, Skills } from '../../api/types.gen.ts'
import { EditingProvider } from '../../components/SettingRow/SettingRow.tsx'
import { SETTINGS_SECTIONS, type SettingsPage } from '../../lib/settings.ts'
import appStyles from '../App.module.css'
import styles from './Settings.module.css'
import { Agents, Connections, Merging, Product, Safety, type Save } from './sections.tsx'

interface Props {
  section: SettingsPage
  settings: Settings
  skills: Skills
  save: Save
}

const ROWS = {
  connections: Connections,
  agents: Agents,
  merging: Merging,
  safety: Safety,
  product: Product,
} as const

/** What a section's rows have in common, under its title. */
const INTRO: Partial<Record<SettingsPage, ReactNode>> = {
  connections: 'Each connection is checked when you save it. A failed check saves nothing.',
  merging:
    "GitHub's required checks and approvals still apply. These settings add what only you approve.",
  product: (
    <>
      Each issue runs its own copy of the product, for agents to check their changes and for Slack's
      Preview button. Each copy gets its own port (<code>$PORT</code>) and data folder.
    </>
  ),
}

/** The settings by what the owner comes to change, one section at a time. */
export function SettingsView({ section, settings, skills, save }: Props) {
  const [editing, setEditing] = useState<string | undefined>()
  const [discarded, setDiscarded] = useState<string | undefined>()
  const shown = useRef(section)
  const changed = useRef<boolean>(false)
  useEffect(() => {
    // Leaving a section with a form open drops what was typed in it; the owner is told.
    if (shown.current !== section) {
      shown.current = section
      setDiscarded(changed.current ? editing : undefined)
      setEditing(undefined)
      changed.current = false
    }
  }, [section, editing])
  const Rows = ROWS[section]
  const title = SETTINGS_SECTIONS.find(([id]) => id === section)?.[1] ?? ''
  return (
    <>
      <h1 className={`${appStyles['title']} ${styles['pageTitle']}`}>Settings</h1>
      <div className={styles['layout']}>
        <nav className={styles['subnav']} aria-label="Settings sections">
          {SETTINGS_SECTIONS.map(([id, name]) => (
            <Link
              key={id}
              to={`/settings/${id}`}
              aria-current={id === section ? 'page' : undefined}
            >
              {name}
            </Link>
          ))}
        </nav>
        <div>
          {settings.problem !== null && (
            <p className={styles['refused']} role="alert">
              The factory cannot use these settings as they are: {settings.problem} Correct it here;
              it applies once saved.
            </p>
          )}
          {discarded !== undefined && (
            <p className={styles['discarded']} role="status">
              Your unsaved change to {discarded} was not saved.
            </p>
          )}
          <section className={styles['section']} aria-labelledby="settings-section">
            <h2 className={styles['sectionTitle']} id="settings-section">
              {title}
            </h2>
            {INTRO[section] !== undefined && <p className={styles['intro']}>{INTRO[section]}</p>}
            <ul className={styles['rows']}>
              <EditingProvider
                value={{
                  current: editing,
                  set: (row) => {
                    setDiscarded(undefined)
                    setEditing(row)
                    changed.current = false
                  },
                  changed: () => {
                    changed.current = true
                  },
                }}
              >
                <Rows settings={settings} skills={skills} save={save} />
              </EditingProvider>
            </ul>
          </section>
        </div>
      </div>
    </>
  )
}
