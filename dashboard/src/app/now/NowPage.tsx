import { useState } from 'react'
import { api } from '../../api/client.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { statusRun } from '../../lib/vocabulary.ts'
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
          const row = overview.data?.issues.find((i) => i.key === key)
          if (row === undefined) {
            return
          }
          api
            .launches({ issue: key })
            .then((rows) => setRun(statusRun(row.now, rows)?.id))
            .catch(() => undefined)
        }}
      />
      {run !== undefined && <RunDrawer id={run} onClose={() => setRun(undefined)} />}
    </>
  )
}
