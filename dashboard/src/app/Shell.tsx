import { useCallback, useEffect, useState } from 'react'
import { Navigate, Outlet, useLocation, useOutletContext } from 'react-router'
import { api } from '../api/client.ts'
import type { Overview, SetupState } from '../api/types.gen.ts'
import { type Polled, usePoll } from '../api/usePoll.ts'
import { AppHeader, type Area } from '../components/AppHeader/AppHeader.tsx'
import { GoalBand } from '../components/GoalBand/GoalBand.tsx'
import styles from './App.module.css'

export interface ShellContext {
  overview: Polled<Overview>
  setup: SetupState
  /** Reads the setup state again, as after the setup is saved. */
  reloadSetup: () => Promise<void>
}

export function useShell(): ShellContext {
  return useOutletContext<ShellContext>()
}

/** The header area a page belongs to: an issue and its prototype belong to Now. */
function area(path: string): Area | undefined {
  if (path === '/' || path.startsWith('/issues')) {
    return 'now'
  }
  return (['activity', 'events', 'sprints', 'settings'] as const).find((a) =>
    path.startsWith(`/${a}`)
  )
}

export function Shell() {
  const [setup, setSetup] = useState<SetupState | undefined>()
  const [failed, setFailed] = useState(false)
  const overview = usePoll('overview', api.overview)
  const version = usePoll('version', api.version)
  const { pathname } = useLocation()

  const reloadSetup = useCallback(async (): Promise<void> => {
    try {
      setSetup(await api.setupState())
      setFailed(false)
    } catch {
      setFailed(true)
    }
  }, [])
  useEffect(() => {
    reloadSetup().catch(() => undefined)
  }, [reloadSetup])
  useEffect(() => {
    if (!failed) {
      return undefined
    }
    const timer = window.setTimeout(() => {
      reloadSetup().catch(() => undefined)
    }, 5000)
    return (): void => window.clearTimeout(timer)
  }, [failed, reloadSetup])

  if (setup !== undefined && setup.step !== 'done' && pathname !== '/setup') {
    return <Navigate to="/setup" replace />
  }
  const done = setup?.step === 'done'
  return (
    <>
      <AppHeader
        repository={setup?.repository}
        version={version.data}
        current={area(pathname)}
        navigation={done}
      />
      {done && pathname !== '/setup' && overview.data !== undefined && (
        <GoalBand goal={overview.data.goal} />
      )}
      <main className={styles['page']}>
        {failed && (
          <p className={styles['notice']} role="alert">
            The dashboard cannot reach the factory. Run <kbd>ovenspring start</kbd> in a terminal in
            this repository's folder.
          </p>
        )}
        {/* Once set up, Now's status says what the configuration stops; setup has no status. */}
        {!done && setup?.error != null && (
          <p className={styles['notice']} role="alert">
            The configuration cannot be used, so no work starts: {setup.error}
          </p>
        )}
        {setup !== undefined && (
          <Outlet context={{ overview, setup, reloadSetup } satisfies ShellContext} />
        )}
      </main>
    </>
  )
}
