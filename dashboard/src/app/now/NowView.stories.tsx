import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { missing, NOW, paused, problem, working } from '../../fixtures/overview.ts'
import { clock } from '../../lib/time.ts'
import { NowView } from './NowView.tsx'

const meta = {
  component: NowView,
  args: { overview: working, now: NOW, onFollow: fn() },
} satisfies Meta<typeof NowView>

export default meta
type Story = StoryObj<typeof meta>

/** Golden state 1: a decision waits, two issues are in progress, two wait, two are done. */
export const Working: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent('Working')
    const yours = canvas.getByRole('region', { name: 'Waiting for you' })
    await expect(within(yours).getByRole('link', { name: /Approve prototype/ })).toHaveAttribute(
      'target',
      '_blank'
    )
    const progress = canvas.getByRole('region', { name: 'In progress' })
    await expect(progress).toHaveTextContent('ReviewJudging findings3 min')
    await expect(canvas.getByRole('region', { name: 'Waiting to start' })).toHaveTextContent(
      'After ACME-142 merges'
    )
    await expect(canvas.getByRole('region', { name: 'Done' })).toHaveTextContent(
      'Closed: already done'
    )
    await userEvent.click(within(progress).getByRole('button', { name: 'Judging findings' }))
    await expect(args.onFollow).toHaveBeenCalledWith('ACME-141')
    await expect(within(progress).getByText('Waiting for CI')).toBeVisible()
    await expect(within(progress).queryByRole('button', { name: 'Waiting for CI' })).toBeNull()
  },
}

/** Golden state 2: the lead has a problem with one issue; the rest is unchanged. */
export const Problem: Story = {
  args: { overview: problem },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/Problem: CI fix stopped/)).toBeVisible()
  },
}

/** Golden state 3: paused, with the cause and how to resume; issues under way are on hold. */
export const Paused: Story = {
  args: { overview: paused },
  globals: { theme: 'dark' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent('Paused')
    await expect(
      canvas.getByText(
        "The agent on ACME-141 (implement) stopped with an error: You've hit your usage limit. Nothing new starts until you resume."
      )
    ).toBeVisible()
    await expect(canvas.getAllByText('On hold (paused)')).toHaveLength(2)
  },
}

/** Golden state 4: two prerequisites missing, each with its fix and where. */
export const PrerequisitesMissing: Story = {
  args: { overview: missing },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent('Partly on hold')
    await expect(
      canvas.getByRole('link', { name: 'Change the Slack tokens in Settings' })
    ).toHaveAttribute('href', '/settings/connections')
    await expect(
      canvas.getByRole('link', { name: 'Change the Linear key in Settings' })
    ).toHaveAttribute('href', '/settings/connections')
  },
}

/** The dashboard lost contact: the last data stays, and the title says so. */
export const LostContact: Story = {
  args: { lostAt: new Date('2026-11-21T14:18:00Z') },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent(
      'The dashboard lost contact with the factory'
    )
    await expect(
      canvas.getByText(`Last contact ${clock('2026-11-21T14:18:00Z')}.`, { exact: false })
    ).toBeVisible()
    await expect(canvas.getByRole('region', { name: 'In progress' })).toBeVisible()
  },
}

/** Between sprints: the lead proposes from the open issues it names, and the last sprint's end. */
export const Proposing: Story = {
  args: {
    overview: {
      ...working,
      sprint: null,
      decisions: [],
      issues: [],
      status: { ...working.status, state: 'proposing' },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent(
      'Proposing the next sprint'
    )
    await expect(
      canvas.getByText(/the goal and the 9 open issues on Linear\. The proposal will reach you/)
    ).toBeVisible()
    await expect(
      canvas.getByText(/^No sprint running\. The last sprint ended Nov 2\d, \d{2}:\d{2}\.$/)
    ).toBeVisible()
  },
}

/** GitHub out of reach: since when, what waits on it, and where its own status is. */
export const Unreachable: Story = {
  args: {
    overview: {
      ...working,
      status: {
        ...working.status,
        state: 'unreachable',
        unreachable: { service: 'github', since: '2026-11-21T14:02:00Z', why: 'server-error' },
      },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent("Can't reach GitHub")
    await expect(
      canvas.getByText(/every call to GitHub fails \(errors on its side\)/)
    ).toHaveTextContent(/Steps that need GitHub \(pushing, CI, merging\) wait/)
    await expect(canvas.getByRole('link', { name: /githubstatus\.com/ })).toHaveAttribute(
      'href',
      'https://www.githubstatus.com'
    )
  },
}

/** No open issue serves the goal: the lead's message in Slack is a link away. */
export const WaitingForIssues: Story = {
  args: {
    overview: {
      ...working,
      sprint: null,
      decisions: [],
      issues: [],
      status: { ...working.status, state: 'waiting-for-issues' },
      toldWaiting: {
        at: '2026-11-21T15:44:00Z',
        link: 'https://acme.slack.test/archives/C01/p9',
      },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByRole('link', { name: /the lead told you in Slack what is missing/ })
    ).toHaveAttribute('href', 'https://acme.slack.test/archives/C01/p9')
    await expect(canvas.getByText(/None of the 9 open issues on Linear/)).toBeVisible()
  },
}

/** The lead found nothing to propose because no issue is open at all. */
export const WaitingWithNoOpenIssue: Story = {
  args: {
    overview: {
      ...working,
      sprint: null,
      decisions: [],
      issues: [],
      openIssues: 0,
      status: { ...working.status, state: 'waiting-for-issues' },
      toldWaiting: null,
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByText(/^No issue is open on Linear; the lead told you in Slack what is missing\.$/)
    ).toBeVisible()
  },
}

const ended = working.issues.filter((i) => i.now.section === 'done')
const applying = {
  ...working,
  sprint: working.sprint && { ...working.sprint, done: ended.length, total: ended.length },
  decisions: [],
  issues: ended,
  status: { ...working.status, state: 'applying-agreements' as const },
  agreements: 'https://github.com/acme/storefront/pull/212',
}

/** A sprint over two days: an issue that ended on an earlier day shows its date with the time. */
export const DoneOnAnEarlierDay: Story = {
  args: {
    overview: {
      ...working,
      sprint: working.sprint && { ...working.sprint, startedAt: '2026-11-20T09:05:00Z' },
      issues: working.issues.map((i) =>
        i.now.section === 'done' && i.key === 'ACME-146'
          ? { ...i, now: { ...i.now, endedAt: '2026-11-20T12:00:00Z' } }
          : i
      ),
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/^Nov 2[01], \d{2}:\d{2}$/)).toBeVisible()
  },
}

/** Every issue ended; the factory merges the working agreements once GitHub allows it. */
export const MergingTheAgreements: Story = {
  args: { overview: applying },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent(
      'Merging the working agreements'
    )
    await expect(canvas.getByRole('link', { name: /See the pull request/ })).toHaveAttribute(
      'href',
      'https://github.com/acme/storefront/pull/212'
    )
  },
}

/** `merge.by = owner`: the working agreements wait for the owner to merge them. */
export const AgreementsWithTheOwner: Story = {
  args: {
    overview: {
      ...applying,
      decisions: [
        {
          issue: null,
          ask: 'merge-on-github',
          title: 'Working agreements and knowledge',
          link: 'https://github.com/acme/storefront/pull/212',
          delivered: true,
        },
      ],
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent('Waiting for you')
    await expect(canvas.getByRole('link', { name: /Merge on GitHub/ })).toHaveAttribute(
      'href',
      'https://github.com/acme/storefront/pull/212'
    )
  },
}

/** The factory merges, but GitHub requires the owner's approving review first. */
export const AgreementsAwaitingApproval: Story = {
  args: {
    overview: {
      ...applying,
      decisions: [
        {
          issue: null,
          ask: 'approve-on-github',
          title: 'Working agreements and knowledge',
          link: 'https://github.com/acme/storefront/pull/212',
          delivered: true,
        },
      ],
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByText(/^Approve the working agreements and knowledge on GitHub\./)
    ).toBeVisible()
    await expect(canvas.getByRole('link', { name: /Approve on GitHub/ })).toHaveAttribute(
      'href',
      'https://github.com/acme/storefront/pull/212'
    )
  },
}

/** The lead writes the sprint's purpose in Markdown; its inline code reads as code. */
export const PurposeWithInlineCode: Story = {
  args: {
    overview: {
      ...working,
      sprint: working.sprint && {
        ...working.sprint,
        purpose: 'Every built-in profile can be chosen through the `profile` input',
      },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const code = canvas.getByText('profile', { selector: 'code' })
    await expect(code).toBeVisible()
    await expect(canvas.queryByText(/`profile`/)).toBeNull()
  },
}

/** A pull request GitHub still blocks for a rule the factory cannot read waits in Merge. */
export const BlockedByGitHub: Story = {
  args: {
    overview: {
      ...working,
      issues: working.issues.map((i) =>
        i.key === 'ACME-144' ? { ...i, now: { ...i.now, activity: 'blocked-on-git-hub' } } : i
      ),
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const row = canvas.getByText('Match search words despite typos').closest('li')
    if (row === null) {
      throw new Error('the issue has no row')
    }
    await expect(within(row).getByText('Blocked by GitHub')).toBeVisible()
  },
}
