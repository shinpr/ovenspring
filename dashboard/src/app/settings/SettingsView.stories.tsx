import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { settings, skills } from '../../fixtures/settings.ts'
import { SettingsView } from './SettingsView.tsx'

const meta = {
  component: SettingsView,
  args: { section: 'agents', settings, skills, save: fn(async () => ({})) },
} satisfies Meta<typeof SettingsView>

export default meta
type Story = StoryObj<typeof meta>

/** Golden state 7: Agents, with the reviewer's form open on Codex; one form opens at a time. */
export const Agents: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Change Reviewer' }))
    const reviewer = canvas.getByRole('form', { name: 'Reviewer' })
    await expect(within(reviewer).getByRole('radio', { name: 'Codex' })).toBeChecked()
    await expect(within(reviewer).getByRole('combobox', { name: 'Effort' })).toHaveValue('high')
    await userEvent.click(canvas.getByRole('button', { name: 'Change Lead' }))
    await expect(canvas.queryByRole('form', { name: 'Reviewer' })).toBeNull()
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(args.save).toHaveBeenCalledWith('roles', {
      role: 'lead',
      cli: 'claude',
      model: 'claude-opus-5-5',
      effort: 'high',
      skills: [],
      append: '',
    })
  },
}

/** Merging: what is always held is fixed; each category held by path is its own row. */
export const Merging: Story = {
  args: { section: 'merging' },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.queryByRole('button', { name: 'Change Always held for you' })).toBeNull()
    await userEvent.click(canvas.getByRole('button', { name: 'Change CI changes' }))
    await userEvent.type(
      canvas.getByRole('textbox', { name: 'Path patterns, one per line' }),
      '.github/**'
    )
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(args.save).toHaveBeenCalledWith('general', {
      baseBranch: 'main',
      mergeBy: 'factory',
      held: { schema: ['db/migrations/**'], ci: ['.github/**'] },
      personReviews: { payments: 'How customers are charged or refunded' },
      agreementsPath: '.ovenspring/agreements',
      knowledgePath: '.ovenspring/knowledge',
      githubToken: null,
    })
  },
}

/** Agents: the working agreements and knowledge folders follow the repository's own layout. */
export const Folders: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    for (const [name, saved, entered, change] of [
      [
        'Working agreements folder',
        '.ovenspring/agreements',
        'docs/factory/agreements',
        { agreementsPath: 'docs/factory/agreements' },
      ],
      [
        'Knowledge folder',
        '.ovenspring/knowledge',
        'docs/knowledge',
        { knowledgePath: 'docs/knowledge' },
      ],
    ] as const) {
      await expect(canvas.getByText(saved)).toBeVisible()
      await userEvent.click(canvas.getByRole('button', { name: `Change ${name}` }))
      const folder = canvas.getByRole('textbox', { name: 'Folder in the repository' })
      await userEvent.clear(folder)
      await userEvent.type(folder, entered)
      await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
      await expect(args.save).toHaveBeenCalledWith('general', expect.objectContaining(change))
    }
  },
}

/** Changes a person reviews: the owner names each and says, in their own words, what it covers. */
export const PersonReviews: Story = {
  args: { section: 'merging' },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('payments', { selector: 'b' })).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Change Changes a person reviews' }))
    await userEvent.type(
      canvas.getByRole('textbox', { name: /a name, a colon/ }),
      '{enter}auth: sign-in, sessions and who may see what'
    )
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(args.save).toHaveBeenCalledWith('general', {
      baseBranch: 'main',
      mergeBy: 'factory',
      held: { schema: ['db/migrations/**'] },
      personReviews: {
        payments: 'How customers are charged or refunded',
        auth: 'sign-in, sessions and who may see what',
      },
      agreementsPath: '.ovenspring/agreements',
      knowledgePath: '.ovenspring/knowledge',
      githubToken: null,
    })
  },
}

/** Safety: the commands always denied are said before the owner's own. */
export const Safety: Story = {
  args: { section: 'safety' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/^Always denied:/)).toHaveTextContent(
      "Always denied: git push, git commit, gh pr merge, approving a pull request, and GitHub's merge and review endpoints."
    )
    await expect(canvas.getByText(/^Review rounds:/)).toHaveTextContent('Review rounds: no limit')
    await expect(canvas.getByText(/^CI fixes:/)).toHaveTextContent('CI fixes: no limit')
  },
}

/** Running the product: the same choice as setup; a change that does not start shows its output. */
export const Product: Story = {
  args: {
    section: 'product',
    save: fn(async () => ({
      check: 'the app exited before it was ready; its log ends with:\nmissing script: dev',
    })),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('pnpm dev --port $PORT')).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Change Commands' }))
    const start = canvas.getByRole('textbox', { name: 'Start' })
    await userEvent.clear(start)
    await userEvent.type(start, 'pnpm start')
    await userEvent.click(canvas.getByRole('button', { name: 'Check and save' }))
    await expect(args.save).toHaveBeenCalledWith('app', {
      runs: 'commands',
      setup: 'pnpm install',
      start: 'pnpm start',
      ready: null,
    })
    await expect(await canvas.findByText(/missing script: dev/)).toBeVisible()
    await userEvent.click(canvas.getByRole('radio', { name: /There is nothing to run/ }))
    await userEvent.click(canvas.getByRole('button', { name: 'Check and save' }))
    await expect(args.save).toHaveBeenLastCalledWith('app', { runs: 'nothing' })
  },
}

/**
 * Connections: GitHub names the repository and the account it acts as; a saved key is named by how
 * it ends, kept unless replaced, and replaced on request.
 */
export const SavedKey: Story = {
  args: { section: 'connections' },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/^Repository/)).toHaveTextContent(
      "Repository acme/storefront (this folder's origin)"
    )
    await expect(canvas.getByText(/^Acting as/)).toHaveTextContent(
      'Acting as acme-factory-bot, with the signed-in gh'
    )
    await userEvent.click(canvas.getByRole('button', { name: 'Change Issue tracker' }))
    const form = within(canvas.getByRole('form', { name: 'Issue tracker' }))
    await expect(form.getByText(/Saved, ending in fj39\./)).toBeVisible()
    await userEvent.click(form.getByRole('button', { name: 'Replace API key' }))
    await expect(form.getByLabelText('API key')).toHaveFocus()
    await userEvent.type(form.getByLabelText('API key'), 'lin_api_typo')
    await userEvent.click(form.getByRole('button', { name: 'Keep the saved one' }))
    await expect(form.getByRole('button', { name: 'Replace API key' })).toHaveFocus()
    await userEvent.click(form.getByRole('button', { name: 'Check and save' }))
    await expect(args.save).toHaveBeenLastCalledWith('tracker', {
      kind: 'linear',
      team: 'ACME',
      filter: '',
      apiKey: null,
    })

    await userEvent.click(canvas.getByRole('button', { name: 'Change Issue tracker' }))
    const reopened = within(canvas.getByRole('form', { name: 'Issue tracker' }))
    await userEvent.click(reopened.getByRole('button', { name: 'Replace API key' }))
    await userEvent.type(reopened.getByLabelText('API key'), 'lin_api_new0001')
    await userEvent.type(reopened.getByLabelText('Filter command'), './.ovenspring/filter-issue.sh')
    await userEvent.click(reopened.getByRole('button', { name: 'Check and save' }))
    await expect(args.save).toHaveBeenLastCalledWith('tracker', {
      kind: 'linear',
      team: 'ACME',
      filter: './.ovenspring/filter-issue.sh',
      apiKey: 'lin_api_new0001',
    })
  },
}

/**
 * Golden state 15: Connections with the Error notices row open; the saved choice has the focus and
 * the arrow keys move it.
 */
export const ErrorNotices: Story = {
  args: { section: 'connections' },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(
      canvas.getByText('A message in Slack when an error starts and when it clears.')
    ).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: 'Change Error notices' }))
    const group = canvas.getByRole('group', { name: 'Error notices' })
    await expect(within(group).getByRole('radio', { name: 'On (default)' })).toHaveFocus()
    await userEvent.keyboard('{ArrowDown}')
    await expect(within(group).getByRole('radio', { name: 'Off' })).toBeChecked()
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(args.save).toHaveBeenCalledWith('notices', { errors: false })
  },
}
