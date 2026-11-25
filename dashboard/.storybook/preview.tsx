import type { Preview } from '@storybook/react-vite'
import { MemoryRouter } from 'react-router'
import '../src/styles/index.ts'

const preview: Preview = {
  globalTypes: {
    theme: {
      description: 'Dashboard theme',
      toolbar: { title: 'Theme', items: ['light', 'dark'], dynamicTitle: true },
    },
  },
  initialGlobals: { theme: 'light' },
  parameters: {
    // Every story must pass axe (WCAG 2.2 AA) in the theme it renders in.
    a11y: { test: 'error', options: { runOnly: ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa'] } },
  },
  beforeEach: () => {
    window.localStorage.clear()
  },
  decorators: [
    (Story, { globals }) => {
      document.documentElement.setAttribute(
        'data-theme',
        globals['theme'] === 'dark' ? 'dark' : 'light'
      )
      return (
        <MemoryRouter>
          <Story />
        </MemoryRouter>
      )
    },
  ],
}

export default preview
