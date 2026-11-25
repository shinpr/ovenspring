import { useState } from 'react'
import { useParams } from 'react-router'
import { api } from '../../api/client.ts'
import { usePoll } from '../../api/usePoll.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { saveFile } from '../../lib/download.ts'
import { RunDrawer } from '../RunDrawer.tsx'
import { IssueView } from './IssueView.tsx'

export function IssuePage() {
  const { key = '' } = useParams()
  const { data, answeredAt, failed } = usePoll(`issue:${key}`, async () => ({
    issue: await api.issue(key),
    runs: await api.launches({ issue: key }),
  }))
  const [run, setRun] = useState<string | undefined>()
  const contact = <Contact what="This issue" answeredAt={answeredAt} failed={failed} />
  if (data === undefined) {
    return contact
  }
  return (
    <>
      {contact}
      <IssueView
        issue={data.issue}
        runs={data.runs}
        now={new Date()}
        onOpenRun={setRun}
        onDownload={() => saveFile(`issue:${key}`)}
        onCancel={(words) => api.cancelIssue(key, words)}
      />
      {run !== undefined && <RunDrawer id={run} onClose={() => setRun(undefined)} />}
    </>
  )
}
