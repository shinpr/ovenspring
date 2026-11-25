import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { SettingRow } from './SettingRow.tsx'

const meta = {
  component: SettingRow,
  args: {
    name: 'Base branch',
    what: "Where each issue's branch starts and merges.",
    summary: 'main',
    mono: true,
    onOpen: fn(),
    onSave: fn(async () => ({})),
    fields: ['repo.base_branch'],
    children: (reasons) => (
      <label>
        Base branch{' '}
        <input
          defaultValue="main"
          aria-invalid={reasons['repo.base_branch'] === undefined ? undefined : true}
        />
        {reasons['repo.base_branch'] !== undefined && <span>{reasons['repo.base_branch']}</span>}
      </label>
    ),
  },
  decorators: [
    (Story) => (
      <ul style={{ listStyle: 'none', padding: 0 }}>
        <Story />
      </ul>
    ),
  ],
} satisfies Meta<typeof SettingRow>

export default meta
type Story = StoryObj<typeof meta>

/** Change resets the draft and moves focus into the form; Cancel returns it to Change. */
export const OpenAndCancel: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    const edit = canvas.getByRole('button', { name: 'Change Base branch' })
    await userEvent.click(edit)
    await expect(args.onOpen).toHaveBeenCalledOnce()
    await expect(canvas.getByRole('textbox')).toHaveFocus()
    await userEvent.click(canvas.getByRole('button', { name: 'Cancel' }))
    await expect(canvas.queryByRole('form')).toBeNull()
    await expect(canvas.getByRole('button', { name: 'Change Base branch' })).toHaveFocus()
  },
}

/** A refused save keeps the editor open, with each reason under its field or under the buttons. */
export const Refused: Story = {
  args: {
    initiallyOpen: true,
    onSave: fn(async () => ({
      'repo.base_branch': 'Enter the base branch.',
      general: 'No branch `trunk` on origin.',
    })),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(await canvas.findByText('Enter the base branch.')).toBeVisible()
    await expect(canvas.getByRole('textbox', { name: /Base branch/ })).toHaveFocus()
    await expect(canvas.getByRole('alert')).toHaveTextContent('No branch `trunk` on origin.')
    await expect(canvas.getByRole('form', { name: 'Base branch' })).toBeVisible()
  },
}

export const Saved: Story = {
  args: { initiallyOpen: true },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(args.onSave).toHaveBeenCalledOnce()
    await expect(canvas.queryByRole('form')).toBeNull()
  },
}

/** A save that fails outright says so, and leaves the form usable again. */
export const SaveFailed: Story = {
  args: {
    initiallyOpen: true,
    onSave: fn(async () => {
      throw new Error('network')
    }),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(await canvas.findByRole('button', { name: 'Save' })).toBeEnabled()
    await expect(canvas.getByRole('alert')).toHaveTextContent('It could not be saved. Try again.')
  },
}

/** A fixed setting shows its value with no way to change it. */
export const Fixed: Story = {
  render: () => <SettingRow name="Always held for you" summary="Changes a design says need you." />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.queryByRole('button')).toBeNull()
  },
}
