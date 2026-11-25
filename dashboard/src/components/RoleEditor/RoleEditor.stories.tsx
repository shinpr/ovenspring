import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { settings, skills } from '../../fixtures/settings.ts'
import { RoleEditor } from './RoleEditor.tsx'

const meta = {
  component: RoleEditor,
  args: {
    role: 'lead',
    config: settings.config.roles.lead,
    skills,
    onSave: fn(async () => ({})),
    open: true,
  },
  decorators: [
    (Story) => (
      <ul style={{ listStyle: 'none', padding: 0 }}>
        <Story />
      </ul>
    ),
  ],
} satisfies Meta<typeof RoleEditor>

export default meta
type Story = StoryObj<typeof meta>

/** Moving the lead to Codex clears the model and effort set for Claude Code and says so; Codex's
 * effort is chosen from its levels, and its repository skills are offered. */
export const MovedToCodex: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText(/No skills there yet/)).toBeVisible()
    await userEvent.click(canvas.getByRole('radio', { name: 'Codex' }))
    await expect(
      canvas.getByText(/model and effort set for Claude Code do not apply to Codex/)
    ).toBeVisible()
    await userEvent.selectOptions(canvas.getByRole('combobox', { name: 'Effort' }), 'xhigh')
    await userEvent.click(canvas.getByRole('checkbox', { name: /payments-domain/ }))
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(args.onSave).toHaveBeenCalledWith({
      role: 'lead',
      cli: 'codex',
      model: null,
      effort: 'xhigh',
      skills: ['payments-domain'],
      append: '',
    })
  },
}

/** OpenCode takes its provider's model and variant as text; a skill it cannot load is unselected
 * with a note. */
export const OpenCodeChosen: Story = {
  args: { role: 'reviewer', config: settings.config.roles.reviewer },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('radio', { name: 'OpenCode' }))
    await expect(canvas.getByRole('textbox', { name: 'Effort' })).toHaveValue('')
    await userEvent.click(canvas.getByRole('radio', { name: 'Claude Code' }))
    await expect(canvas.getByText(/is not in .claude\/skills/)).toHaveTextContent(
      'Note: payments-domain is not in .claude/skills, where Claude Code loads skills, so it is unselected.'
    )
  },
}

export const SkillRefused: Story = {
  args: {
    role: 'reviewer',
    config: settings.config.roles.reviewer,
    onSave: fn(async () => ({
      'roles.reviewer.skills':
        "payments-domain is not in .agents/skills, where this role's agent loads skills. Put it there, or uncheck it.",
    })),
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }))
    await expect(await canvas.findByText(/payments-domain is not in .agents\/skills/)).toBeVisible()
  },
}
