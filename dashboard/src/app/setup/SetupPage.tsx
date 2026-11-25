import { useEffect } from 'react'
import { Navigate } from 'react-router'
import { ApiError, api } from '../../api/client.ts'
import type { SetupState } from '../../api/types.gen.ts'
import { usePoll } from '../../api/usePoll.ts'
import { useShell } from '../Shell.tsx'
import { ConnectView } from './ConnectView.tsx'
import { GoalStepView } from './GoalStepView.tsx'
import { RunView } from './RunView.tsx'
import { SetupFrame } from './SetupFrame.tsx'

/** Setup opens on its first step not done, as the server reads it from what is saved. */
export function SetupPage() {
  const { setup, reloadSetup, overview } = useShell()
  switch (setup.step) {
    case 'done':
      return <Navigate to="/" replace />
    case 'connect':
      return (
        <SetupFrame repository={setup.repository} step={1}>
          <ConnectView
            manifest={setup.slackManifest}
            origin={setup.origin}
            onCheck={api.checkSetup}
            onSave={async (request) => {
              await api.saveSetup(request)
              await reloadSetup()
            }}
          />
        </SetupFrame>
      )
    case 'run':
      return <RunStep setup={setup} reloadSetup={reloadSetup} />
    case 'goal':
      return (
        <SetupFrame repository={setup.repository} step={3}>
          <GoalStepView
            onStart={async (text) => {
              try {
                await api.saveGoal(text)
              } catch (e) {
                return e instanceof ApiError
                  ? (e.errors['text'] ?? 'The goal could not be saved.')
                  : 'The goal could not be saved.'
              }
              await reloadSetup()
              overview.refresh()
              return undefined
            }}
          />
        </SetupFrame>
      )
  }
}

/** Run the product, asking again while the lead reads and the factory starts the product. */
function RunStep({ setup, reloadSetup }: { setup: SetupState; reloadSetup: () => Promise<void> }) {
  const polled = usePoll('setup', api.setupState)
  const run = polled.data?.run ?? setup.run
  // Settled elsewhere (in Settings, or the file): setup moves on.
  const moved = polled.data !== undefined && polled.data.step !== 'run'
  useEffect(() => {
    if (moved) {
      reloadSetup().catch(() => undefined)
    }
  }, [moved, reloadSetup])
  return (
    <SetupFrame repository={setup.repository} step={2}>
      {run !== null && (
        <RunView
          run={run}
          onSave={async (request) => {
            await api.saveRun(request)
            await reloadSetup()
          }}
        />
      )}
    </SetupFrame>
  )
}
