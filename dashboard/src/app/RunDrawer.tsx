import { api } from '../api/client.ts'
import { usePoll } from '../api/usePoll.ts'
import { RunPanel } from '../components/RunPanel/RunPanel.tsx'

/** The run panel for launch `id`, read again while it is open so a running run keeps talking. */
export function RunDrawer({ id, onClose }: { id: string; onClose: () => void }) {
  const { data, failed } = usePoll(`launch:${id}`, () => api.launch(id))
  const run = data ?? (failed ? null : undefined)
  return <RunPanel run={run} now={new Date()} logUrl={api.logUrl(id)} onClose={onClose} />
}
