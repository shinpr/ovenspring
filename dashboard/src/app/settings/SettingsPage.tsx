import { useCallback, useEffect, useState } from 'react'
import { Navigate, useParams } from 'react-router'
import { ApiError, api } from '../../api/client.ts'
import type { Settings, Skills } from '../../api/types.gen.ts'
import { SETTINGS_SECTIONS } from '../../lib/settings.ts'
import { useShell } from '../Shell.tsx'
import { SettingsView } from './SettingsView.tsx'

const NOT_SAVED = { request: 'The setting could not be saved. Try again.' }

export function SettingsPage() {
  const { overview } = useShell()
  const { section } = useParams()
  const [loaded, setLoaded] = useState<{ settings: Settings; skills: Skills } | undefined>()
  const [failed, setFailed] = useState<string | undefined>()
  const load = useCallback(async (): Promise<void> => {
    try {
      const [settings, skills] = await Promise.all([api.settings(), api.skills()])
      setLoaded({ settings, skills })
      setFailed(undefined)
    } catch (e) {
      setFailed(
        e instanceof ApiError && e.status === 409
          ? `The configuration file cannot be read: ${e.errors['request'] ?? 'it is not set up'}. Fix it in .ovenspring/config.toml.`
          : 'The settings could not be read.'
      )
    }
  }, [])
  useEffect(() => {
    load().catch(() => undefined)
  }, [load])
  const page = SETTINGS_SECTIONS.find(([id]) => id === section)?.[0]
  if (page === undefined) {
    return <Navigate to="/settings/connections" replace />
  }
  if (loaded === undefined) {
    return failed === undefined ? null : <p role="alert">{failed}</p>
  }
  return (
    <SettingsView
      section={page}
      settings={loaded.settings}
      skills={loaded.skills}
      save={async (part, body) => {
        try {
          const settings = await api.saveSettings(part, body)
          setLoaded((l) => (l === undefined ? l : { ...l, settings }))
          overview.refresh()
          return {}
        } catch (e) {
          return e instanceof ApiError && Object.keys(e.errors).length > 0 ? e.errors : NOT_SAVED
        }
      }}
    />
  )
}
