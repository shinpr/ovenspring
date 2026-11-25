import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { ApiError } from '../../api/client.ts'
import {
  checking,
  failed,
  nothing,
  reading,
  started,
  stopped,
  unknown,
} from '../../fixtures/setup.ts'
import { RunView } from './RunView.tsx'
import { SetupFrame } from './SetupFrame.tsx'

const meta = {
  component: RunView,
  decorators: [
    (Story) => (
      <SetupFrame repository="storefront" step={2}>
        <Story />
      </SetupFrame>
    ),
  ],
  args: { run: started, onSave: fn(async () => undefined) },
} satisfies Meta<typeof RunView>

export default meta
type Story = StoryObj<typeof meta>

/** While the lead reads: where it is and what it has said, with nothing to press. */
export const Reading: Story = {
  args: { run: reading },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('The lead is reading the repository')).toBeVisible()
    await expect(canvas.getByText(/The dev script starts Next.js/)).toBeVisible()
    await expect(canvas.queryByRole('button')).toBeNull()
  },
}

/** The factory starts the lead's commands once. */
export const Checking: Story = {
  args: { run: checking },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('Starting it once to check')).toBeVisible()
    await expect(canvas.getByText('pnpm dev --port $PORT')).toBeVisible()
  },
}

/** Golden state 8 (Run part): the product started; Continue keeps the commands as they are. */
export const Started: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByText('Started on port 4101 and answered after 1 min 12 s.')
    ).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(args.onSave).toHaveBeenCalledWith({
      runs: 'commands',
      setup: 'pnpm install --frozen-lockfile',
      start: 'pnpm dev --port $PORT',
      ready: 'http://localhost:$PORT/health',
    })
  },
}

/** A failed start shows what it printed; changed commands that fail again show the new output. */
export const DidNotStart: Story = {
  args: {
    run: failed,
    onSave: fn(async () => {
      throw new ApiError(400, {
        check: '`app.setup` failed (exit status: 1); its output ends with:\nnot found',
      })
    }),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/ECONNREFUSED 127.0.0.1:5432/)).toBeVisible()
    const start = canvas.getByRole('textbox', { name: 'Start' })
    await userEvent.clear(start)
    await userEvent.type(start, 'pnpm start --port $PORT')
    await expect(canvas.getByText(/Changed. The factory starts it once/)).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(await canvas.findByText(/`app.setup` failed/)).toBeVisible()
  },
}

/** Nothing to run: the lead's reading, and what it means. */
export const NothingToRun: Story = {
  args: { run: nothing },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('radio', { name: /There is nothing to run/ })).toBeChecked()
    await expect(canvas.getByText(/describes the package as a library/)).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(args.onSave).toHaveBeenCalledWith({ runs: 'nothing' })
  },
}

/** A save the factory did not answer says so whichever choice was made, not "did not start". */
export const SaveFailed: Story = {
  args: {
    run: nothing,
    onSave: fn(async () => {
      throw new Error('network')
    }),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(await canvas.findByRole('alert')).toHaveTextContent(
      'It could not be saved: the factory did not answer. Try again.'
    )
    await expect(canvas.queryByText('Did not start.')).toBeNull()
  },
}

/** The lead could not tell: the owner enters the commands, and Start is needed. */
export const Unknown: Story = {
  args: { run: unknown },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('The lead could not tell how it starts.')).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Continue' }))
    await expect(canvas.getByRole('textbox', { name: 'Start' })).toHaveAccessibleDescription(
      /Enter the command that starts the product/
    )
    await expect(args.onSave).not.toHaveBeenCalled()
  },
}

/** The lead's agent could not run: its own error, and how the factory goes on. */
export const AgentStopped: Story = {
  args: { run: stopped },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText("The lead's agent stopped with an error.")).toBeVisible()
    await expect(canvas.getByText(/401 Unauthorized/)).toBeVisible()
  },
}
