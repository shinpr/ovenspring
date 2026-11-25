import { api } from '../../api/client.ts'
import { usePoll } from '../../api/usePoll.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { saveFile } from '../../lib/download.ts'
import { useShell } from '../Shell.tsx'
import { SprintsView } from './SprintsView.tsx'

export function SprintsPage() {
  const { overview } = useShell()
  const current = overview.data?.sprint ?? null
  const id = current?.id
  const { data, answeredAt, failed } = usePoll(`sprints:${id ?? ''}`, async () => ({
    ended: await api.sprints(),
    runs: id === undefined ? [] : await api.launches({ sprint: id }),
  }))
  const contact = <Contact what="The sprints" answeredAt={answeredAt} failed={failed} />
  if (data === undefined) {
    return contact
  }
  return (
    <>
      {contact}
      <SprintsView
        current={current}
        runs={data.runs}
        ended={data.ended}
        onDownload={(sprint) => saveFile(`sprint:${sprint}`)}
      />
    </>
  )
}
