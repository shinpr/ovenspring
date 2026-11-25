import { useNavigate } from 'react-router'
import { ApiError, api } from '../../api/client.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { useShell } from '../Shell.tsx'
import { GoalView } from './GoalView.tsx'

export function GoalPage() {
  const { overview } = useShell()
  const navigate = useNavigate()
  if (overview.data === undefined) {
    return <Contact what="The goal" answeredAt={undefined} failed={overview.failed} />
  }
  return (
    <GoalView
      goal={overview.data.goal}
      onCancel={() => {
        Promise.resolve(navigate(-1)).catch(() => undefined)
      }}
      onSave={async (text) => {
        try {
          await api.saveGoal(text)
          overview.refresh()
          await navigate('/')
          return undefined
        } catch (e) {
          return e instanceof ApiError && e.errors['text'] !== undefined
            ? e.errors['text']
            : 'The goal could not be saved. Try again.'
        }
      }}
    />
  )
}
