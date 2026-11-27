// The settings the golden states show, as the server answers them.

import type { Settings, Skills } from '../api/types.gen.ts'

export const settings: Settings = {
  config: {
    repo: { base_branch: 'main' },
    merge: { by: 'factory' },
    held: { schema: { paths: ['db/migrations/**'] } },
    person_reviews: { payments: 'How customers are charged or refunded' },
    agreements: { path: '.ovenspring/agreements' },
    knowledge: { path: '.ovenspring/knowledge' },
    notices: { errors: true },
    agents: { permissions: 'auto', mcp: ['context7'] },
    roles: {
      lead: { cli: 'claude', model: 'claude-opus-5-5', effort: 'high' },
      executor: { cli: 'claude' },
      reviewer: { cli: 'codex', model: 'gpt-5.5', effort: 'high', skills: ['payments-domain'] },
      analyzer: { cli: 'claude', append: 'Count merged pull requests by week.' },
    },
    tracker: { kind: 'linear', team: 'ACME' },
    slack: { channel: 'C0123456789', owners: ['U0123456789'] },
    app: { setup: 'pnpm install', start: 'pnpm dev --port $PORT' },
    guard: { deny: ['terraform apply *'] },
    review: { bots: ['ai-reviewer[bot]'] },
    limits: { issues_at_once: 3 },
  },
  problem: null,
  alwaysDenied: [
    { kind: 'command', text: 'git push' },
    { kind: 'command', text: 'git commit' },
    { kind: 'command', text: 'gh pr merge' },
    { kind: 'action', text: 'approving a pull request' },
    { kind: 'action', text: "GitHub's merge and review endpoints" },
  ],
  held: [
    { credential: 'slack-bot-token', ending: 'Q7mT' },
    { credential: 'slack-app-token', ending: 'c2Vb' },
    { credential: 'linear-api-key', ending: 'fj39' },
  ],
  repository: 'acme/storefront',
  githubAccount: { login: 'acme-factory-bot', fromGh: false },
  mcp: [
    { name: 'context7', source: "this repository's .mcp.json", reaches: [] },
    { name: 'github', source: 'your Claude Code configuration', reaches: ['github'] },
  ],
}

export const skills: Skills = {
  claude: { dirs: ['.claude/skills'], skills: [] },
  codex: {
    dirs: ['.agents/skills'],
    skills: [{ name: 'payments-domain', description: 'Payment states and refunds' }],
  },
  opencode: {
    dirs: ['.agents/skills', '.claude/skills'],
    skills: [{ name: 'payments-domain', description: 'Payment states and refunds' }],
  },
}
