import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import type { EventsPage } from '../../api/types.gen.ts'
import { Contact } from '../../components/Contact/Contact.tsx'
import { EVENTS, EVENTS_NOW, eventsPage, type Situation } from '../../fixtures/events.ts'
import { EventsView } from './EventsView.tsx'

/** What the page shows first in each of the prototype's situations. */
const first = (situation: Situation): EventsPage =>
  eventsPage(EVENTS[situation], new URLSearchParams())

const meta = {
  component: EventsView,
  args: {
    page: first('open'),
    unanswered: false,
    show: 'all',
    issue: '',
    onFilter: fn(),
    onOlder: fn(),
    now: EVENTS_NOW,
  },
} satisfies Meta<typeof EventsView>

export default meta
type Story = StoryObj<typeof meta>

/** Golden state 9: two errors going on; a folded start, a clear, two drops and a filter failure. */
export const TwoErrorsGoingOn: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    const now = canvas.getByRole('region', { name: 'Errors now' })
    await expect(now).toHaveTextContent("Can't reach GitHub")
    await expect(now).toHaveTextContent('23 times, last at')
    await expect(now).toHaveTextContent('The issue filter fails')
    const did = canvas.getByRole('region', { name: 'What the factory did' })
    await expect(did).toHaveTextContent(
      'The issue filter could not judge ACME-153: ./.ovenspring/filter-issue.sh exited with 127 (jq: command not found). The issue goes on as without a filter.'
    )
    await expect(did).toHaveTextContent('Happened 23 times so far, last at')
    await expect(
      canvas.getAllByRole('link', { name: 'Linear issue (opens in a new tab)' })
    ).toHaveLength(2)
    const sprintLinks = canvas.getAllByRole('link', {
      name: `Sprint: ${'Shoppers can find a product by what it does, not only by its name.'}`,
    })
    await expect(sprintLinks).toHaveLength(2)
    for (const link of sprintLinks) {
      await expect(link).toHaveAttribute('href', '/sprints#sprint-20261127-0902')
    }
    await expect(within(did).getAllByRole('listitem')).toHaveLength(15)
    await userEvent.click(canvas.getByRole('button', { name: 'Show older events' }))
    await expect(args.onOlder).toHaveBeenCalled()
  },
}

/** Golden state 10: both errors cleared, each with its Cleared line. */
export const NothingWrongNow: Story = {
  args: { page: first('clear') },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('Nothing is wrong now.')).toBeVisible()
    await expect(canvas.getByText('GitHub answers again.')).toBeVisible()
    await expect(canvas.getByText('The issue filter works again.')).toBeVisible()
    await expect(canvas.getAllByText('Cleared').length).toBeGreaterThanOrEqual(2)
  },
}

/** Golden state 11: notices off; the errors after the switch were not told. */
export const NoticesOff: Story = {
  args: { page: first('off') },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByText(/Error notices are off, so Slack tells you nothing about these\./)
    ).toBeVisible()
    await expect(canvas.getByRole('link', { name: 'Turn them on in Settings' })).toBeVisible()
    await expect(canvas.getAllByText(/Not told: error notices are off\./)).toHaveLength(2)
    await expect(canvas.getByText('Settings changed: Error notices are now off.')).toBeVisible()
  },
}

/** Golden state 12: nothing recorded yet. */
export const NothingRecorded: Story = {
  args: { page: first('new') },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('Nothing has been recorded yet.')).toBeVisible()
    await expect(canvas.getByText('Nothing is wrong now.')).toBeVisible()
  },
}

/** Golden state 13: the server could not read the events. */
export const Unreadable: Story = {
  args: { page: first('unreadable') },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByText('The events could not be read. The dashboard tries again in a few seconds.')
    ).toBeVisible()
    await expect(canvas.getByText('Not known while the events cannot be read.')).toBeVisible()
  },
}

/** Golden state 14: the dashboard lost contact; the page stays as it was last read. */
export const LostContact: Story = {
  args: { page: first('lost') },
  render: (args) => (
    <>
      <Contact what="These events" answeredAt={EVENTS_NOW} failed />
      <EventsView {...args} />
    </>
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('status')).toHaveTextContent(
      /^The dashboard lost contact with the factory\. This page is as it was at \d{2}:\d{2}\.$/
    )
    await expect(canvas.getByText("Can't reach GitHub")).toBeVisible()
  },
}

/** Nothing answered yet: the title alone, under Contact's line on the page. */
export const NotReadYet: Story = {
  args: { page: undefined, unanswered: true },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('heading', { level: 1 })).toHaveTextContent('Events')
    await expect(canvas.queryByRole('region', { name: 'Errors now' })).toBeNull()
  },
}

const open = first('open')
const [newest, ...rest] = open.lines

/** A repeat on another issue, a notice still waiting, a sprint S-03 does not list. */
export const RepeatWaitingAndUnlistedSprint: Story = {
  args: {
    page: {
      ...open,
      lines: [
        ...(newest === undefined
          ? []
          : [
              {
                ...newest,
                kind: 'filter-failed',
                fields: {
                  issue: 'ACME-144',
                  error: 'exited with 2',
                  command: './.ovenspring/filter-issue.sh',
                },
                issue: { key: 'ACME-144', known: true },
                error: {
                  role: 'repeat' as const,
                  key: 'filter' as const,
                  cause: 'exited with 2',
                  times: 1,
                  last: newest.at,
                  startedAt: '2026-11-27T14:05:00Z',
                },
              },
            ]),
        {
          seq: 900,
          at: '2026-11-27T12:00:00Z',
          kind: 'sprint-begun',
          fields: { sprint: '20261128-0900' },
          issue: null,
          sprint: { id: '20261128-0900', purpose: null, listed: false },
          error: null,
        },
        {
          seq: 899,
          at: '2026-11-27T11:00:00Z',
          kind: 'error-cleared',
          fields: { failure: { key: 'filter', since: '2026-11-27T10:00:00Z' }, removed: true },
          issue: null,
          sprint: null,
          error: {
            role: 'cleared',
            key: 'filter',
            startedAt: '2026-11-27T10:00:00Z',
            times: 2,
            removed: true,
            told: { state: 'waiting' },
          },
        },
        ...rest.slice(0, 3),
      ],
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/Same error as at .+, so no new message\./)).toBeVisible()
    await expect(canvas.getByText('Sprint begun Nov 28, 09:00')).toBeVisible()
    await expect(canvas.getByText('The issue filter was removed.')).toBeVisible()
    await expect(
      canvas.getByText(
        'Not told yet: Slack did not accept the message. It is sent once Slack accepts messages again.'
      )
    ).toBeVisible()
  },
}
