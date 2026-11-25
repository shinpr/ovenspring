import { useState } from 'react'
import { api } from '../../api/client.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { RunDrawer } from '../RunDrawer.tsx'
import { useShell } from '../Shell.tsx'
import { NowView } from './NowView.tsx'

export function NowPage() {
  const { overview } = useShell()
  const [run, setRun] = useState<string | undefined>()
  if (overview.data === undefined) {
    return <Contact what="The factory's state" answeredAt={undefined} failed={overview.failed} />
  }
  return (
    <>
      <NowView
        overview={overview.data}
        now={new Date()}
        lostAt={overview.failed ? overview.answeredAt : undefined}
        onFollow={(key) => {
          api
            .launches({ issue: key })
            .then((rows) => {
              // The run under way; a run that ended just now is the one its activity named.
              const running = rows.find((r) => r.result === 'running') ?? rows[0]
              setRun(running?.id)
            })
            .catch(() => undefined)
        }}
      />
      {run !== undefined && <RunDrawer id={run} onClose={() => setRun(undefined)} />}
    </>
  )
}
