import { Link, useParams } from 'react-router'
import { api } from '../../api/client.ts'
import { usePoll } from '../../api/usePoll.ts'
import { Breadcrumb } from '../../components/Breadcrumb/Breadcrumb.tsx'
import { Contact } from '../../components/Contact/Contact.tsx'
import { ExternalLink } from '../../components/ExternalLink/ExternalLink.tsx'
import appStyles from '../App.module.css'
import styles from './Prototype.module.css'

const STRENGTH = {
  binding: 'Binding: the build follows this page as drawn, unless the UI Spec says otherwise.',
  reference: 'Reference: the build takes what this page shows, and decides the rest.',
} as const

/**
 * The issue's prototype, as the lead wrote it. An agent wrote its page, so it runs in a sandbox
 * without the dashboard's origin: its scripts run, but cannot reach the dashboard or its cookie.
 */
export function PrototypePage() {
  const { key = '' } = useParams()
  const issue = encodeURIComponent(key)
  const { data, answeredAt, failed } = usePoll(`issue:${key}`, () => api.issue(key))
  return (
    <>
      <Contact what="This prototype's issue" answeredAt={answeredAt} failed={failed} />
      <Breadcrumb>
        <Link to={`/issues/${issue}`}>{key}</Link> / Prototype
      </Breadcrumb>
      <div className={styles['head']}>
        <h1 className={appStyles['title']}>
          Prototype{data === undefined ? '' : `: ${data.title}`}
        </h1>
      </div>
      {data?.prototype != null && <p className={styles['strength']}>{STRENGTH[data.prototype]}</p>}
      {data?.decision?.ask === 'approve-prototype' && data.decision.link !== null && (
        <p className={styles['approve']}>
          <ExternalLink href={data.decision.link}>Approve or reply in Slack</ExternalLink>{' '}
          <span className={styles['muted']}>You approve it in Slack.</span>
        </p>
      )}
      <iframe
        className={styles['frame']}
        title={`Prototype of ${key}`}
        sandbox="allow-scripts"
        src={`/api/v1/issues/${issue}/prototype`}
      />
    </>
  )
}
