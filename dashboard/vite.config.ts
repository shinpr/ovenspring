import { storybookTest } from '@storybook/addon-vitest/vitest-plugin'
import react from '@vitejs/plugin-react'
import { playwright } from '@vitest/browser-playwright'
import { defineConfig, type TestProjectInlineConfiguration } from 'vitest/config'

/** Every story rendered in a real browser and checked by axe, once per theme. */
function storybookProject(theme: 'light' | 'dark'): TestProjectInlineConfiguration {
  return {
    extends: true as const,
    plugins: [storybookTest({ configDir: '.storybook' })],
    test: {
      name: `storybook-${theme}`,
      provide: { theme },
      setupFiles: ['.storybook/vitest.setup.ts'],
      browser: {
        enabled: true,
        headless: true,
        provider: playwright(),
        instances: [{ browser: 'chromium' as const }],
      },
    },
  }
}

export default defineConfig({
  plugins: [react()],
  build: { outDir: 'dist', emptyOutDir: true },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'node',
          include: ['tokens/**/*.test.ts', 'src/**/*.test.ts'],
          environment: 'node',
        },
      },
      storybookProject('light'),
      storybookProject('dark'),
    ],
  },
})
