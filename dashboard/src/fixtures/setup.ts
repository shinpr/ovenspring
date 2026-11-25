import type { SetupRun } from '../api/types.gen.ts'

/** The lead's commands for a web shop, as `run-commands` answers. */
const COMMANDS = {
  runs: 'commands',
  setup: 'pnpm install --frozen-lockfile',
  start: 'pnpm dev --port $PORT',
  ready: 'http://localhost:$PORT/health',
  reason: 'README.md starts the shop with pnpm dev, and GET /health answers once it is up.',
} as const

const run: SetupRun = {
  said: [],
  stopped: null,
  mention: 'Oven Spring',
  found: null,
  checking: false,
  check: null,
}

export const reading: SetupRun = {
  ...run,
  said: [
    'Reading README.md and package.json to see how the shop runs on a laptop.',
    'The dev script starts Next.js; it takes --port, so the start command passes $PORT.',
  ],
}

export const checking: SetupRun = { ...reading, found: COMMANDS, checking: true }

export const started: SetupRun = {
  ...run,
  found: COMMANDS,
  check: { started: { port: 4101, seconds: 72 } },
}

export const failed: SetupRun = {
  ...run,
  found: COMMANDS,
  check: {
    failed: {
      output:
        "the app did not answer http://localhost:4101/health within 60s; its log ends with:\nError: connect ECONNREFUSED 127.0.0.1:5432\nPrismaClientInitializationError: Can't reach database server at localhost:5432",
    },
  },
}

export const nothing: SetupRun = {
  ...run,
  found: {
    runs: 'nothing',
    setup: null,
    start: null,
    ready: null,
    reason:
      'package.json has no start or dev script, and README.md describes the package as a library other projects install.',
  },
}

export const unknown: SetupRun = {
  ...run,
  found: {
    runs: 'unknown',
    setup: null,
    start: null,
    ready: null,
    reason: 'The README says to ask the team for the staging credentials before starting it.',
  },
}

export const stopped: SetupRun = {
  ...run,
  stopped:
    'the agent on the sprint (run-commands) stopped with an error: 401 Unauthorized. Your access token could not be refreshed; please log in again',
}
