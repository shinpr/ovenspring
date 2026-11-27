import { useEffect, useRef } from 'react'
import { useLocation } from 'react-router'
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
  const { hash } = useLocation()
  // The hash scrolled to already: the sprint it names may arrive with either poll, the ended
  // sprints with this page's or "This sprint" with the overview, so each render tries until one
  // holds it, and later polls leave the page where the owner moved it.
  const scrolled = useRef('')
  useEffect(() => {
    if (hash === scrolled.current) {
      return
    }
    // Sprint ids are digits and dashes, so a fragment is never decoded: one that names nothing
    // here, malformed or not, leaves the list as it is.
    const target = hash === '' ? null : document.getElementById(hash.slice(1))
    if (target !== null) {
      target.scrollIntoView()
      scrolled.current = hash
    }
  })
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
