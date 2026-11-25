import { defineConfig, devices } from '@playwright/test'

const ci = process.env['CI'] !== undefined

// The fixture-e2e lane: the production bundle (the one the binary embeds) in a real browser, its
// API answered from fixtures. `pnpm run build` comes first; `vite preview` only serves `dist`.
export default defineConfig({
  testDir: 'e2e',
  testMatch: '**/*.fixture.e2e.test.ts',
  fullyParallel: true,
  forbidOnly: ci,
  use: {
    ...devices['Desktop Chrome'],
    baseURL: 'http://127.0.0.1:4173',
    viewport: { width: 1280, height: 800 },
    locale: 'en-US',
    timezoneId: 'UTC',
  },
  webServer: {
    command: 'pnpm exec vite preview --host 127.0.0.1 --port 4173 --strictPort',
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: false,
  },
})
