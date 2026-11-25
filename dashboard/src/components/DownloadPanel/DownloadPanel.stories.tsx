import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { DownloadPanel } from './DownloadPanel.tsx'

const meta = {
  component: DownloadPanel,
  args: { what: "this sprint's records", onDownload: fn(async () => undefined) },
} satisfies Meta<typeof DownloadPanel>

export default meta
type Story = StoryObj<typeof meta>

/** Opening the panel shows what the file holds and its Download button. */
export const Opened: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByText("Download this sprint's records"))
    await expect(canvas.getByText(/Secrets are masked/)).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Download (.jsonl)' }))
    await expect(args.onDownload).toHaveBeenCalledOnce()
  },
}

/** While the file is prepared, the line beside the button says so and the button takes no second press. */
export const Preparing: Story = {
  args: { open: true, onDownload: fn(() => new Promise<void>(() => undefined)) },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const download = canvas.getByRole('button', { name: 'Download (.jsonl)' })
    await userEvent.click(download)
    await expect(canvas.getByRole('status')).toHaveTextContent('Preparing…')
    await expect(download).toBeDisabled()
  },
}

export const Failed: Story = {
  args: {
    open: true,
    onDownload: fn(async () => {
      throw new Error('500')
    }),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Download (.jsonl)' }))
    await expect(await canvas.findByText('The download failed. Try again.')).toBeVisible()
  },
}
