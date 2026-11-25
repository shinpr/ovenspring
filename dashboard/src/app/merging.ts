import type { Choice } from '../components/Choices/Choices.tsx'

/** Who merges a pull request once it may be merged, as setup and settings offer it. */
export const MERGERS: ReadonlyArray<Choice<'factory' | 'owner'>> = [
  {
    value: 'factory',
    label: 'The factory',
    hint: 'Once checks and every approval are in.',
  },
  {
    value: 'owner',
    label: 'You',
    hint: 'The factory tells you in Slack when a pull request is ready.',
  },
]
