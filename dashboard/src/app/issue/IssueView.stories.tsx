import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { inReview, runs } from '../../fixtures/issue.ts'
import { NOW } from '../../fixtures/overview.ts'
import { IssueView } from './IssueView.tsx'

const meta = {
  component: IssueView,
  args: {
    issue: inReview,
    runs,
    now: NOW,
    onOpenRun: fn(),
    onDownload: fn(async () => undefined),
    onCancel: fn(async () => undefined),
  },
} satisfies Meta<typeof IssueView>

export default meta
type Story = StoryObj<typeof meta>

/** Golden state 5: an issue in Review, its current phase open with its runs. */
export const InReview: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent(inReview.title)
    await expect(canvas.getByText(/is judging findings/)).toHaveTextContent(
      'Lead is judging findings · for 3 min follow'
    )
    // The line's own "follow" comes before the path's.
    const follows = canvas.getAllByRole('button', { name: 'follow' })
    await expect(follows).toHaveLength(2)
    await expect(canvas.getAllByText(/Code review, round 1/)).not.toHaveLength(0)
    await expect(canvas.queryByText(/code-review round/)).toBeNull()
    const path = canvas.getByRole('region', { name: 'Path' })
    await expect(within(path).getByText(/^now/)).toBeVisible()
    await userEvent.click(within(path).getByRole('button', { name: 'follow' }))
    await expect(args.onOpenRun).toHaveBeenCalledWith(runs[0]?.id)
    await expect(within(path).getByText(/^Next: the executor corrects/)).toBeVisible()
    await expect(canvas.getByText('Medium')).toBeVisible()
    await expect(canvas.getByText('3 findings: 2 applied, 1 being judged.')).toBeVisible()
    await expect(
      canvas.getByText('5 runs, 1 running. $1.31 so far; 2 have no known cost.')
    ).toBeVisible()
    const opener = canvas.getByRole('button', { name: 'Cancel issue…' })
    await expect(canvas.queryByRole('region', { name: 'Cancel this issue' })).toBeNull()
    await userEvent.click(opener)
    await expect(opener).toHaveAttribute('aria-expanded', 'true')
    await expect(canvas.getByText(/marks it canceled on Linear/)).toBeVisible()
  },
}

/** A cancel already sent stays sent when its panel is closed and opened again before the page shows the issue ended. */
export const CancelSentThenReopened: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    const opener = canvas.getByRole('button', { name: 'Cancel issue…' })
    await userEvent.click(opener)
    await userEvent.click(canvas.getByRole('button', { name: 'Cancel the issue' }))
    await expect(args.onCancel).toHaveBeenCalledTimes(1)
    await userEvent.click(opener)
    await userEvent.click(opener)
    await expect(canvas.getByRole('button', { name: 'Cancel the issue' })).toBeDisabled()
    await expect(canvas.getByText('Canceled. The work stops within seconds.')).toBeVisible()
  },
}

/** Waiting for the owner: the action and where to take it. */
export const WaitingForYou: Story = {
  args: {
    issue: {
      ...inReview,
      now: { ...inReview.now, section: 'you', activity: null, since: null },
      decision: {
        issue: inReview.key,
        ask: 'approve-merge',
        title: inReview.title,
        link: 'https://acme.slack.test/archives/C01/p2',
        delivered: true,
      },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('Waiting for you: Approve merge')).toBeVisible()
    await expect(canvas.getByRole('link', { name: /Answer in Slack/ })).toBeVisible()
  },
}

/** Waiting to start while as many issues as Settings allows are in progress. */
export const WaitingForAnIssueToEnd: Story = {
  args: {
    runs: [],
    issue: {
      ...inReview,
      pullRequest: null,
      scale: null,
      scaleEvidence: [],
      outcome: null,
      nonGoals: [],
      path: inReview.path.map((p) => ({
        ...p,
        state: 'ahead' as const,
        startedAt: null,
        endedAt: null,
      })),
      findings: [],
      now: {
        ...inReview.now,
        section: 'waiting-to-start',
        phase: null,
        activity: null,
        since: null,
        wait: { reason: 'place', heldBy: ['ACME-142', 'ACME-144'] },
      },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getAllByText(/^Starts when one of ACME-142, ACME-144 ends/)).toHaveLength(1)
    await expect(canvas.getByText('No runs yet.')).toBeVisible()
    await expect(canvas.queryByText('See them in Activity')).toBeNull()
    await expect(canvas.getByRole('link', { name: 'Settings' })).toHaveAttribute(
      'href',
      '/settings/safety'
    )
  },
}

/** An issue closed as already done: the phases it never reached say why, not what they hold. */
export const ClosedFirst: Story = {
  args: {
    issue: {
      ...inReview,
      scale: null,
      now: { ...inReview.now, section: 'done', ended: 'closed', endedAt: '2026-11-21T12:00:00Z' },
      path: inReview.path.map((p) =>
        p.phase === 'requirements' ? p : { ...p, state: 'skipped', startedAt: null, endedAt: null }
      ),
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getAllByText('Not needed: the issue was closed.')).toHaveLength(4)
    await expect(canvas.queryByText(/The executor implements each task/)).toBeNull()
    await expect(canvas.queryByRole('button', { name: 'Cancel issue…' })).toBeNull()
    await expect(canvas.getByText('Not judged: the issue ended first.')).toBeVisible()
  },
}

/** The owner canceled with words that already end a sentence: the line keeps one period. */
export const CanceledWithWords: Story = {
  args: {
    issue: {
      ...inReview,
      canceled: 'Billing takes this over.',
      now: { ...inReview.now, section: 'done', ended: 'canceled', endedAt: '2026-11-21T12:00:00Z' },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByText(/^Canceled by you at .*: Billing takes this over\.$/)
    ).toBeVisible()
  },
}

/** Waiting to start after one dependency merges. */
export const WaitingForADependency: Story = {
  args: {
    runs: [],
    issue: {
      ...inReview,
      now: {
        ...inReview.now,
        section: 'waiting-to-start',
        phase: null,
        activity: null,
        since: null,
        wait: { reason: 'after', keys: ['ACME-142'] },
      },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('Starts after ACME-142 merges.')).toBeVisible()
  },
}

/** A small change: Design is skipped and says why. */
export const SmallChange: Story = {
  args: {
    issue: {
      ...inReview,
      scale: 'small',
      path: inReview.path.map((p) =>
        p.phase === 'design' ? { ...p, state: 'skipped', startedAt: null, endedAt: null } : p
      ),
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('A small change goes straight to building.')).toBeVisible()
  },
}
