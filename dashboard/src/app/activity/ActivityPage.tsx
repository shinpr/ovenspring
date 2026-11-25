import { useEffect, useState } from 'react'
import { useSearchParams } from 'react-router'
import { api } from '../../api/client.ts'
import type { EndedSprint } from '../../api/types.gen.ts'
import { usePoll } from '../../api/usePoll.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { days } from '../../lib/time.ts'
import { unmarked } from '../../lib/typed.tsx'
import { RunDrawer } from '../RunDrawer.tsx'
import { useShell } from '../Shell.tsx'
import { ActivityView, type Option } from './ActivityView.tsx'

const ALL = 'all'

export function ActivityPage() {
  const { overview } = useShell()
  const [params, setParams] = useSearchParams()
  const [ended, setEnded] = useState<EndedSprint[]>([])
  const [run, setRun] = useState<string | undefined>()
  useEffect(() => {
    api.sprints().then(setEnded, () => undefined)
  }, [])
  const current = overview.data?.sprint ?? null
  const sprint = params.get('sprint') ?? current?.id ?? ALL
  const issue = params.get('issue') ?? ''
  const polled = usePoll(`launches:${sprint}:${issue}`, () =>
    api.launches({
      ...(sprint === ALL ? {} : { sprint }),
      ...(issue === '' ? {} : { issue }),
    })
  )
  const sprints: Option[] = [
    ...(current === null
      ? []
      : [{ value: current.id, label: `This sprint: ${unmarked(current.purpose)}` }]),
    ...ended.map((s) => ({
      value: s.id,
      label:
        s.startedAt === null
          ? unmarked(s.purpose)
          : `${days(s.startedAt, s.endedAt)}: ${unmarked(s.purpose)}`,
    })),
    { value: ALL, label: 'All sprints' },
  ]
  const issues: Option[] =
    sprint === current?.id
      ? (overview.data?.issues ?? []).map((i) => ({ value: i.key, label: `${i.key} ${i.title}` }))
      : []
  if (issue !== '' && !issues.some((o) => o.value === issue)) {
    issues.push({ value: issue, label: issue })
  }
  return (
    <>
      <Contact what="The runs" answeredAt={polled.answeredAt} failed={polled.failed} />
      <ActivityView
        runs={polled.data}
        sprints={sprints}
        issues={issues}
        sprint={sprint}
        issue={issue}
        onFilter={(f) => {
          setParams(
            Object.fromEntries(
              Object.entries({ sprint: f.sprint, issue: f.issue }).filter(([, v]) => v !== '')
            )
          )
        }}
        onOpenRun={setRun}
        now={new Date()}
      />
      {run !== undefined && <RunDrawer id={run} onClose={() => setRun(undefined)} />}
    </>
  )
}
