// The words the dashboard shows for each code the API returns. The glance layer (Now) uses short
// fixed words; the read layer (Issue, Activity, Events) uses sentences that name who acts.

import type {
  Activity,
  Ask,
  Cause,
  Cli,
  DocumentKind,
  Ending,
  ErrorKey,
  ErrorNow,
  EventLine,
  JsonValue,
  LaunchRow,
  Phase,
  Position,
  Preparation,
  Role,
  RunResult,
  Scale,
  Step,
  TrackerKind,
} from '../api/types.gen.ts'

export const PHASE: Record<Phase, string> = {
  requirements: 'Requirements',
  design: 'Design',
  build: 'Build',
  review: 'Review',
  merge: 'Merge',
}

const ACTIVITY: Record<Activity, string> = {
  'reading-the-issue': 'Reading the issue',
  'writing-the-prd': 'Writing the PRD',
  'reviewing-the-prd': 'Reviewing the PRD',
  'reading-the-code': 'Reading the code',
  'choosing-documents': 'Choosing documents',
  'making-a-prototype': 'Making a prototype',
  'reading-the-screens': 'Reading the screens',
  'writing-the-ui-spec': 'Writing the UI Spec',
  'reviewing-the-ui-spec': 'Reviewing the UI Spec',
  'recording-decisions': 'Recording decisions',
  'reviewing-decisions': 'Reviewing decisions',
  'writing-the-design': 'Writing the design',
  'checking-the-design': 'Checking the design',
  'reviewing-the-design': 'Reviewing the design',
  'comparing-designs': 'Comparing designs',
  'writing-test-plans': 'Writing test plans',
  planning: 'Planning',
  'reviewing-the-plan': 'Reviewing the plan',
  'splitting-tasks': 'Splitting tasks',
  implementing: 'Implementing',
  'reviewing-tests': 'Reviewing tests',
  'quality-checks': 'Quality checks',
  committing: 'Committing',
  'reviews-running': 'Reviews running',
  'applying-findings': 'Judging findings',
  rechecking: 'Rechecking',
  pushing: 'Pushing',
  'waiting-for-ci': 'Waiting for CI',
  'fixing-ci': 'Fixing CI',
  'resolving-a-conflict': 'Resolving a conflict',
  'waiting-for-bots': 'Waiting for bots',
  'waiting-for-approval': 'Waiting for a GitHub review',
  'checking-merge': 'Checking merge',
  'blocked-on-git-hub': 'Blocked by GitHub',
}

/** The activities the factory does itself, with no agent at work to follow. */
const FACTORYS_OWN: ReadonlySet<Activity> = new Set<Activity>([
  'committing',
  'pushing',
  'waiting-for-ci',
  'waiting-for-bots',
  'waiting-for-approval',
  'checking-merge',
  'blocked-on-git-hub',
])

/** Whether an agent is at work in this activity, so its run can be followed. */
export function agentAtWork(activity: Activity | null): boolean {
  return activity !== null && !FACTORYS_OWN.has(activity)
}

/** What an issue in progress is doing, in three words or fewer. */
export function activityWords(now: Position): string {
  if (now.task !== null) {
    return `Task ${now.task.number} of ${now.task.of}`
  }
  return now.activity === null ? '' : ACTIVITY[now.activity]
}

export const ASK: Record<Ask, string> = {
  'answer-question': 'Answer a question',
  'approve-prd': 'Approve PRD',
  'approve-prototype': 'Approve prototype',
  'approve-design': 'Approve design',
  'approve-merge': 'Approve merge',
  'merge-on-github': 'Merge on GitHub',
  'approve-on-github': 'Approve on GitHub',
  'approve-recovery': 'Approve recovery',
  'resolve-stop': 'Resolve a stop',
  'approve-sprint-proposal': 'Approve sprint proposal',
  'close-sprint': 'Close the sprint',
}

/** What the lead is finding a way past, after "Problem: ". */
export function problemWords(cause: Cause): string {
  switch (cause.cause) {
    case 'step':
      return `${STEP_NAME[cause.step]} stopped`
    case 'dependency-lost':
      return 'dependency dropped'
    case 'pull-closed':
      return 'PR closed'
    case 'push':
      return 'push refused'
    case 'operation':
      return `${cause.operation} failing`
  }
}

export const ENDING: Record<Ending, string> = {
  merged: 'Merged',
  closed: 'Closed: already done',
  dropped: 'Dropped',
  canceled: 'Canceled by you',
  held: 'Held for the review',
}

export const ROLE: Record<Role, string> = {
  lead: 'Lead',
  executor: 'Executor',
  reviewer: 'Reviewer',
  analyzer: 'Analyzer',
}

export const CLI: Record<Cli, string> = {
  claude: 'Claude Code',
  codex: 'Codex',
  opencode: 'OpenCode',
}

export const SCALE: Record<Scale, string> = {
  small: 'Small',
  medium: 'Medium',
  large: 'Large',
  unresolved: 'Not decided yet',
}

/** Each step as the noun a problem names. */
export const STEP_NAME: Record<Step, string> = {
  requirements: 'Requirements',
  prd: 'PRD',
  'prd-review': 'PRD review',
  'codebase-analysis': 'Code reading',
  'scope-check': 'Scope check',
  prototype: 'Prototype',
  'ui-analysis': 'Screen reading',
  'ui-spec': 'UI Spec',
  'ui-spec-review': 'UI Spec review',
  adr: 'Decision record',
  'adr-review': 'Decision review',
  design: 'Design',
  'design-verify': 'Design check',
  'design-review': 'Design review',
  'design-sync': 'Design comparison',
  'test-design': 'Test plans',
  'work-plan': 'Plan',
  'plan-review': 'Plan review',
  decompose: 'Task split',
  implement: 'Implementation',
  'test-review': 'Test review',
  quality: 'Quality checks',
  'code-review': 'Code review',
  'security-review': 'Security review',
  'rerun-check': 'Recheck',
  'review-response': 'Judging findings',
  'fix-ci': 'CI fix',
  'merge-base': 'Conflict resolution',
  exception: 'Problem handling',
  'sprint-plan': 'Sprint proposal',
  facts: 'Sprint facts',
  'sprint-review': 'Sprint review',
  'agreements-check': 'Agreements check',
  conversation: 'Conversation',
  'run-commands': 'Finding how the product runs',
}

/** What a run of each step did, after the role's name: "Reviewer reviewed the design". */
const STEP_DID: Record<Step, string> = {
  requirements: 'read the issue',
  prd: 'wrote the PRD',
  'prd-review': 'reviewed the PRD',
  'codebase-analysis': 'read the code',
  'scope-check': 'decided which documents it needs',
  prototype: 'made the prototype',
  'ui-analysis': 'read the screens',
  'ui-spec': 'wrote the UI Spec',
  'ui-spec-review': 'reviewed the UI Spec',
  adr: 'recorded the decisions',
  'adr-review': 'reviewed the decisions',
  design: 'wrote the design',
  'design-verify': 'checked the design against the code',
  'design-review': 'reviewed the design',
  'design-sync': 'compared it with the other designs',
  'test-design': 'wrote the test skeletons',
  'work-plan': 'wrote the work plan',
  'plan-review': 'reviewed the work plan',
  decompose: 'split the plan into tasks',
  implement: 'implemented the change',
  'test-review': 'checked the tests',
  quality: 'ran the quality checks',
  'code-review': 'reviewed the code',
  'security-review': 'reviewed security',
  'rerun-check': 'chose what to recheck',
  'review-response': 'judged the findings',
  'fix-ci': 'fixed CI',
  'merge-base': 'resolved a conflict',
  exception: 'looked into a problem',
  'sprint-plan': 'proposed the sprint',
  facts: "gathered the sprint's facts",
  'sprint-review': 'wrote the sprint review',
  'agreements-check': 'checked the working agreements',
  conversation: 'answered you',
  'run-commands': 'found how the product runs',
}

/** What a run of each step is doing while it runs, after "is": "Reviewer is reviewing the design". */
const STEP_DOING: Record<Step, string> = {
  requirements: 'reading the issue',
  prd: 'writing the PRD',
  'prd-review': 'reviewing the PRD',
  'codebase-analysis': 'reading the code',
  'scope-check': 'deciding which documents it needs',
  prototype: 'making the prototype',
  'ui-analysis': 'reading the screens',
  'ui-spec': 'writing the UI Spec',
  'ui-spec-review': 'reviewing the UI Spec',
  adr: 'recording the decisions',
  'adr-review': 'reviewing the decisions',
  design: 'writing the design',
  'design-verify': 'checking the design against the code',
  'design-review': 'reviewing the design',
  'design-sync': 'comparing it with the other designs',
  'test-design': 'writing the test skeletons',
  'work-plan': 'writing the work plan',
  'plan-review': 'reviewing the work plan',
  decompose: 'splitting the plan into tasks',
  implement: 'implementing the change',
  'test-review': 'checking the tests',
  quality: 'running the quality checks',
  'code-review': 'reviewing the code',
  'security-review': 'reviewing security',
  'rerun-check': 'choosing what to recheck',
  'review-response': 'judging the findings',
  'fix-ci': 'fixing CI',
  'merge-base': 'resolving a conflict',
  exception: 'looking into a problem',
  'sprint-plan': 'proposing the sprint',
  facts: "gathering the sprint's facts",
  'sprint-review': 'writing the sprint review',
  'agreements-check': 'checking the working agreements',
  conversation: 'answering you',
  'run-commands': 'finding how the product runs',
}

const plural = (n: number, one: string): string => `${n} ${one}${n === 1 ? '' : 's'}`

type Detail = LaunchRow['detail']

/** What a run did or is doing, by step, where its output names more than its step does. */
const NAMED: Partial<Record<Step, (d: Detail, running: boolean) => string | undefined>> = {
  implement: (d, running) => {
    if (d.task === null) {
      return undefined
    }
    const { number, of, name } = d.task
    const task = `task ${number} of ${of}${name === null ? '' : `: ${name}`}`
    return running ? `is implementing ${task}` : `implemented ${task}`
  },
  'review-response': (d, running) => {
    if (running) {
      return d.judging === null ? undefined : `is judging the ${plural(d.judging, 'finding')}`
    }
    if (d.applied === null || d.declined === null) {
      return undefined
    }
    return `judged the ${plural(d.applied + d.declined, 'finding')}`
  },
  decompose: (d, running) =>
    d.tasks === null || running ? undefined : `split the plan into ${plural(d.tasks, 'task')}`,
  requirements: (d, running) => {
    if (running) {
      return undefined
    }
    if (d.closed) {
      return 'read the issue and found it needs no change'
    }
    return d.scale === null ? undefined : `read the issue and judged it ${SCALE[d.scale]}`
  },
}

const DOCUMENT_NAME: Record<DocumentKind, string> = {
  prototype: 'prototype',
  'ui-spec': 'UI Spec',
  adr: 'ADR',
  'design-doc': 'Design Doc',
}

/** The documents a scope check chose: "prototype, UI Spec, Design Doc", "Design Doc only". */
function documents(chosen: readonly DocumentKind[]): string {
  if (chosen.length === 0) {
    return 'none: a small change goes straight to building'
  }
  const names = chosen.map((d) => DOCUMENT_NAME[d])
  return names.length === 1 ? `${names[0] ?? ''} only` : names.join(', ')
}

/** What a preparation turn did, before the step it prepares: "found nothing to ask you". */
const PREPARED: Record<Preparation, string> = {
  unsettled: 'checked whether to ask you anything',
  'nothing-to-ask': 'found nothing to ask you',
  told: 'told you something',
  asked: 'asked you a question',
}

/** What a preparation turn did or is doing: "asked you a question before reading the code". */
function prepared(run: LaunchRow, preparation: Preparation, running: boolean): string {
  const what = running ? 'is checking whether to ask you anything' : PREPARED[preparation]
  return `${what} before ${STEP_DOING[run.step]}`
}

/** What the run did or is doing, after the role's name. */
function did(run: LaunchRow, running: boolean): string {
  if (run.preparation !== null) {
    return prepared(run, run.preparation, running)
  }
  const base =
    NAMED[run.step]?.(run.detail, running) ??
    (running ? `is ${STEP_DOING[run.step]}` : STEP_DID[run.step])
  return run.detail.round === null ? base : `${base} again (round ${run.detail.round})`
}

/** What a check's pass is called where "pass" says less. */
const PASSED: Partial<Record<Step, string>> = {
  'design-verify': 'consistent',
  'design-sync': 'no conflicts',
}

/** How a run that just did its work came out, by step, where its output says. */
const DONE: Partial<Record<Step, (d: Detail) => string | undefined>> = {
  'review-response': (d) => {
    if (d.applied === null || d.declined === null) {
      return undefined
    }
    const parts = [
      d.applied > 0 ? `${d.applied} applied` : '',
      d.declined > 0 ? `${d.declined} declined` : '',
    ].filter(Boolean)
    return parts.length === 0 ? undefined : parts.join(' · ')
  },
  decompose: (d) => (d.tasks === null ? undefined : plural(d.tasks, 'task')),
  'scope-check': (d) => (d.documents === null ? undefined : documents(d.documents)),
}

/** How a run came out, as its line ends: "3 findings", "pass", "1 applied · 1 declined". */
function outcome(run: LaunchRow): string | undefined {
  switch (run.result) {
    case 'running':
      return undefined
    case 'findings':
      return run.detail.found === null ? 'findings' : plural(run.detail.found, 'finding')
    case 'pass':
      return PASSED[run.step] ?? 'pass'
    case 'failed':
      return 'failed'
    case 'retried':
      return 'stopped: provider overloaded'
    case 'done':
      return DONE[run.step]?.(run.detail)
  }
}

/** A run as one sentence and how it came out: "Reviewer reviewed the code" → "3 findings". */
export function runSentence(run: LaunchRow): { who: string; what: string; result?: string } {
  const result = outcome(run)
  return {
    who: ROLE[run.role],
    what: did(run, run.result === 'running'),
    ...(result === undefined ? {} : { result }),
  }
}

/** The tone a result is shown in. */
export function resultTone(result: RunResult): 'ok' | 'fail' | 'muted' {
  switch (result) {
    case 'pass':
      return 'ok'
    case 'failed':
      return 'fail'
    case 'running':
    case 'done':
    case 'findings':
    case 'retried':
      return 'muted'
  }
}

const HOLDS: Record<Phase, string> = {
  requirements: 'The lead reads the issue and decides what to build and how big it is.',
  design:
    'The lead reads the code, writes the design and plans the tasks. Reviewers check each document.',
  build: "Each task implemented and checked, then the repository's quality checks.",
  review:
    'Code and security reviews. The executor judges the findings and corrects what it applies, then they are checked again.',
  merge: 'Push, CI, review bots and required approvals, then the factory merges it.',
}

/** What comes after the activity under way, within its phase. */
const NEXT: Partial<Record<Activity, string>> = {
  'reading-the-issue': 'the design, or building straight away for a small change.',
  'writing-the-prd': 'its review, then your approval of the PRD.',
  'reviewing-the-prd': 'your approval of the PRD.',
  'reading-the-code': 'the lead decides which documents it needs.',
  'choosing-documents': 'each document it chose and its review, then the work plan and its tasks.',
  'making-a-prototype': 'your approval of the prototype, then the rest of the design.',
  'writing-the-design': 'the design review and checks, then the work plan and its tasks.',
  'reviewing-the-design': 'the design checks, then the test plan, the work plan and its tasks.',
  'checking-the-design': 'the test plan, the work plan and its tasks.',
  planning: 'its review, then the tasks.',
  'reviewing-the-plan': 'the plan split into tasks.',
  implementing: 'the remaining tasks, then the quality checks.',
  'reviewing-tests': 'the remaining tasks, then the quality checks.',
  'quality-checks': 'the code review and security review.',
  'reviews-running': 'the executor judges what the reviews find.',
  'applying-findings': 'the quality checks, then the reviews check again.',
  rechecking: 'the executor judges what the rechecks find, or the issue goes on to merge.',
  pushing: 'CI, the review bots and required approvals, then the merge.',
  'waiting-for-ci': 'the review bots, then the merge.',
  'fixing-ci': 'CI runs again.',
  'resolving-a-conflict': 'the push and CI again.',
  'waiting-for-bots': 'required approvals, then the merge.',
  'waiting-for-approval': 'the merge.',
}

/** What comes after the activity under way, as the line under its phase reads it. */
export function nextInPhase(activity: Activity | null): string | undefined {
  return activity === null ? undefined : NEXT[activity]
}

const SMALL_DESIGN: string = 'A small change goes straight to building.'

/** What a phase ahead on the issue's path holds. */
export function phaseHolds(phase: Phase): string {
  return HOLDS[phase]
}

/** Why a phase was skipped: a small change needs no design; otherwise the issue ended first. */
export function phaseSkipped(phase: Phase, scale: Scale | null, ended: Ending | null): string {
  if (phase === 'design' && scale === 'small') {
    return SMALL_DESIGN
  }
  switch (ended) {
    case 'closed':
    case 'dropped':
      return CLOSED_FIRST
    case 'canceled':
      return 'Not needed: you canceled the issue.'
    case 'merged':
    case 'held':
    case null:
      return NOT_NEEDED
  }
}

const CLOSED_FIRST: string = 'Not needed: the issue was closed.'
const NOT_NEEDED: string = 'Not needed.'

/** `text` ending in a period, unless it already ends a sentence. */
export function asSentence(text: string): string {
  return /[.!?]$/.test(text) ? text : `${text}.`
}

// ---- Events (S-08)

/**
 * A sentence as the read layer shows it: words, and the parts drawn apart from them: a command or
 * what it printed (`code`), someone's own words (`quote`), and the link to Settings > Connections.
 */
export type Part = string | { code: string } | { quote: string } | { settings: string }

/** The words of `parts` as one string, as a screen reader or a test reads them. */
export function plain(parts: readonly Part[]): string {
  return parts
    .map((p) => {
      if (typeof p === 'string') {
        return p
      }
      if ('code' in p) {
        return p.code
      }
      return 'quote' in p ? `“${p.quote}”` : p.settings
    })
    .join('')
}

type Fields = EventLine['fields']

function field(fields: Fields, name: string): string | undefined {
  const v = fields[name]
  return typeof v === 'string' ? v : undefined
}

function numberField(fields: Fields, name: string): number | undefined {
  const v = fields[name]
  return typeof v === 'number' ? v : undefined
}

const TRACKER: Record<TrackerKind, string> = { linear: 'Linear', github: 'GitHub' }

function trackerOf(fields: Fields): string | undefined {
  const kind = field(fields, 'tracker')
  return kind === 'linear' || kind === 'github' ? TRACKER[kind] : undefined
}

function cliOf(key: ErrorKey | undefined, fields: Fields): string | undefined {
  const named = field(fields, 'cli') ?? key?.replace(/^cli-/, '')
  return named === 'claude' || named === 'codex' || named === 'opencode' ? CLI[named] : undefined
}

/** The error a line belongs to: from the server's reading of it, or its own `failure`. */
function errorKey(line: EventLine): ErrorKey | undefined {
  if (line.error !== null) {
    return line.error.key
  }
  const failure = line.fields['failure']
  const key =
    typeof failure === 'object' && failure !== null && !Array.isArray(failure)
      ? failure['key']
      : undefined
  return ERROR_KEYS.find((k) => k === key)
}

const ERROR_KEYS: readonly ErrorKey[] = [
  'github-unreachable',
  'tracker-unreachable',
  'tracker-refused',
  'cli-claude',
  'cli-codex',
  'cli-opencode',
  'filter',
]

/** The issue a line concerns, by key. */
function issueOf(line: EventLine): string {
  return line.issue?.key ?? field(line.fields, 'issue') ?? 'the issue'
}

/** The cause the server read for an error's line, or the error the line records. */
function causeOf(line: EventLine): string {
  return line.error !== null && line.error.role !== 'cleared'
    ? line.error.cause
    : (field(line.fields, 'error') ?? 'no answer')
}

/** How often the factory tries a service again, after "tries again". */
function interval(key: ErrorKey | undefined): ' every minute' | '' {
  return key === 'tracker-unreachable' ? ' every minute' : ''
}

function unreachable(line: EventLine): Part[] {
  const key = errorKey(line)
  const service =
    key === 'tracker-unreachable' ? (trackerOf(line.fields) ?? 'the tracker') : 'GitHub'
  const cause = causeOf(line)
  const operation = field(line.fields, 'operation')
  const doing = operation === undefined ? '' : ` while ${operation}`
  return [`Could not reach ${service}${doing}: ${cause}. The factory tries again${interval(key)}.`]
}

function filterFailed(line: EventLine): Part[] {
  const issue = issueOf(line)
  const error = field(line.fields, 'error') ?? 'failed'
  const command = field(line.fields, 'command')
  const output = field(line.fields, 'output')
  return [
    `The issue filter could not judge ${issue}: `,
    ...(command === undefined ? [] : [{ code: command }, ' ']),
    error,
    ...(output === undefined || output === '' ? [] : [' (', { code: output }, ')']),
    '. The issue goes on as without a filter.',
  ]
}

function cleared(line: EventLine): string {
  const key = errorKey(line)
  const tracker = trackerOf(line.fields) ?? 'The tracker'
  switch (key) {
    case 'github-unreachable':
      return 'GitHub answers again.'
    case 'tracker-unreachable':
      return `${tracker} answers again.`
    case 'tracker-refused':
      return `${tracker} accepts the connection again.`
    case 'cli-claude':
    case 'cli-codex':
    case 'cli-opencode':
      return `${cliOf(key, line.fields) ?? 'The agent CLI'} starts again.`
    case 'filter':
      return line.fields['removed'] === true
        ? 'The issue filter was removed.'
        : 'The issue filter works again.'
    case undefined:
      return 'An error cleared.'
  }
}

const DECISION: Record<string, string> = {
  sprint: 'Proposed the sprint in Slack.',
  spec: 'Asked you a question in Slack.',
  design: 'Asked you in Slack to approve its design.',
  prototype: 'Asked you in Slack to approve its prototype.',
  prd: 'Asked you in Slack to approve its PRD.',
  merge: 'Asked you in Slack to approve its merge.',
  review: 'Sent you the sprint review in Slack.',
  recovery: 'Asked you in Slack to approve a way to recover.',
  stopped: 'Asked you in Slack to resolve a stop.',
}

/** An issue's state change, by the fact that moved it. */
const FACT: Record<string, string> = {
  DependenciesMerged: 'Every issue it depends on is merged.',
  DependencyLost: 'An issue it depends on was held or dropped.',
  Planned: 'The plan is split into tasks.',
  Stubs: 'The quality checks found unfinished work.',
  OwnerAnswered: 'You answered in Slack.',
  OwnerReplied: 'You replied in Slack instead of approving.',
  OwnerHeld: 'You held its merge for the sprint review.',
  TakenUp: 'The sprint takes it up again.',
  Pushed: 'Its branch is pushed.',
  ChecksPassed: 'CI passed.',
  ChecksFailed: 'CI failed.',
  BotWaitEnded: 'The wait for review bots ended.',
  CommentsArrived: 'New comments arrived on its pull request.',
  MergedOnGitHub: 'Its pull request was merged on GitHub.',
  Committed: 'Its work is committed.',
  CommitFailed: 'Its work could not be committed.',
  PushRefused: 'The push was refused.',
  PushConflict: 'The push met a conflict.',
  PullClosed: 'Its pull request was closed.',
  Stopped: 'It stopped: an operation kept failing.',
  Resume: 'It goes on where it stopped.',
  RecoveryFailed: 'The recovery failed.',
  Canceled: 'It was canceled.',
}

/** The step a line names, as a noun: "PRD review". */
function stepNamed(fields: Fields): string {
  const step = field(fields, 'step')
  return Object.entries(STEP_NAME).find(([s]) => s === step)?.[1] ?? 'a step'
}

/** A code in CamelCase as words: "DependenciesMerged" -> "Dependencies merged". */
function camelWords(code: string): string {
  return code
    .replace(/(?<=[a-z])(?=[A-Z])/g, ' ')
    .replace(/ ([A-Z])/g, (_, c: string) => ` ${c.toLowerCase()}`)
}

function issueFact(fields: Fields): string {
  const fact = field(fields, 'fact') ?? ''
  const worded = FACT[fact]
  if (worded !== undefined) {
    return worded
  }
  const from = fields['from']
  const step =
    typeof from === 'object' && from !== null && !Array.isArray(from)
      ? stepNamed({ step: from['step'] ?? null })
      : 'a step'
  switch (fact) {
    case 'Done':
      return `${step} finished.`
    case 'Stuck':
      return `${step} could not finish.`
    case 'Prepared':
      return `${step} is prepared.`
    default:
      return fallbackSentence(camelWords(fact) || 'fact')
  }
}

/** A sprint's state change, by the fact that moved it. */
function sprintFact(fields: Fields): string | undefined {
  const fact = field(fields, 'fact')
  const from = fields['from']
  const stage = typeof from === 'object' && from !== null && !Array.isArray(from) ? from : {}
  switch (fact) {
    case 'Approved':
      return stage['stage'] === 'awaiting-review-approval'
        ? 'You closed the sprint in Slack.'
        : 'You approved the sprint proposal in Slack. The sprint started.'
    case 'PlanDone':
      return 'The lead wrote the sprint proposal.'
    case 'Replied':
      return 'You replied to the sprint in Slack.'
    case 'IssuesSettled':
      return 'Every issue of the sprint has ended.'
    case 'FactsDone':
      return "The sprint's facts are gathered."
    case 'ReviewDone':
      return 'The lead wrote the sprint review.'
    case 'CheckDone':
      return 'The working agreements were checked.'
    case 'AgreementsMerged':
      return "The working agreements' pull request merged."
    case 'Stuck':
      return 'A step of the sprint could not finish.'
    case 'IssueOpened':
      return 'A new issue opened on the tracker.'
    case 'Stopped':
      return 'The sprint stopped: an operation kept failing.'
    case 'ExceptionDone':
      return 'The lead looked into what stopped the sprint.'
    case 'Resume':
      return 'The sprint goes on where it stopped.'
    default:
      return undefined
  }
}

/** A list of names as one phrase: "a, b and c". */
function listed(value: JsonValue | undefined): string | undefined {
  if (!Array.isArray(value) || value.length === 0) {
    return undefined
  }
  const names = value.map(String)
  return names.length === 1
    ? names[0]
    : `${names.slice(0, -1).join(', ')} and ${names.at(-1) ?? ''}`
}

/** The Settings row each setting is changed in, by its path; a table names all its keys. */
const SETTING: Record<string, string> = {
  'limits.issues_at_once': 'Issues at once',
  'limits.review_rounds': 'Review rounds',
  'limits.fix_ci_rounds': 'CI fixes',
  'repo.base_branch': 'Base branch',
  'merge.by': 'Who merges',
  'tracker.filter': 'The issue filter command',
  tracker: 'Issue tracker',
  'slack.owners': 'Who the factory answers',
  slack: 'Slack',
  'agreements.path': 'Working agreements folder',
  'knowledge.path': 'Knowledge folder',
  held: 'Always held for you',
  person_reviews: 'Changes a person reviews',
  'review.bots': 'Review bots',
  'guard.deny': 'Denied commands',
  app: 'Commands',
  agents: 'Agents',
  roles: 'Agents',
}

function settingLabel(path: string): string {
  const table = path.split('.')[0] ?? path
  return SETTING[path] ?? SETTING[table] ?? path
}

/** One recorded change of a setting, as `config-reloaded.changes` holds it. */
function settingChanged(change: JsonValue): string {
  if (typeof change !== 'object' || change === null || Array.isArray(change)) {
    return ''
  }
  const path = typeof change['setting'] === 'string' ? change['setting'] : ''
  const value = change['value']
  if (path === 'notices.errors') {
    return typeof value === 'boolean'
      ? `Error notices are now ${value ? 'on' : 'off'}.`
      : 'Error notices were changed.'
  }
  const label = settingLabel(path)
  if (typeof value === 'boolean') {
    return `${label} is now ${value ? 'on' : 'off'}.`
  }
  return typeof value === 'number' ? `${label} is now ${value}.` : `${label} was changed.`
}

function settingsChanged(fields: Fields): string {
  const changes = fields['changes']
  const each = Array.isArray(changes) ? changes.map(settingChanged).filter(Boolean) : []
  return each.length === 0 ? 'Settings changed.' : `Settings changed: ${each.join(' ')}`
}

/** A kind with no sentence of its own: its code with spaces, as a sentence ("Replay reset."). */
function fallbackSentence(kind: string): string {
  const words = kind.replace(/-/g, ' ').trim()
  return asSentence(`${words.charAt(0).toUpperCase()}${words.slice(1)}`)
}

/** What a line of one kind says, from its fields. */
type Sentence = (f: Fields, line: EventLine) => Part[]

function said(sentence: string): Sentence {
  return (): Part[] => [sentence]
}

function failedWith(before: string, after = '.'): Sentence {
  return (f): Part[] => [`${before}${field(f, 'error') ?? ''}${after}`]
}

/** The sentence of each kind the factory records; a kind not here reads as its code. */
const SENTENCE: Record<string, Sentence> = {
  'service-unreachable': (_, line) => unreachable(line),
  'tracker-error': (f, line) =>
    errorKey(line) === 'tracker-refused' || field(f, 'issue') === undefined
      ? [
          `${trackerOf(f) ?? 'The tracker'} refused the connection (${causeOf(line)}). No new issue or change is read from it until it is fixed.`,
        ]
      : [
          `${trackerOf(f) ?? 'The tracker'} refused a change to the issue: ${field(f, 'error') ?? ''}.`,
        ],
  'launch-error': (f, line) => [
    `${cliOf(errorKey(line), f) ?? 'The agent'} could not start a run: `,
    { code: field(f, 'error') ?? '' },
    '.',
  ],
  'filter-failed': (_, line) => filterFailed(line),
  'error-cleared': (_, line) => [cleared(line)],
  'filter-dropped': (f, line) => [
    `The issue filter dropped ${issueOf(line)}: `,
    { quote: field(f, 'reason') ?? '' },
  ],
  pushed: (f) => {
    const pr = numberField(f, 'pr')
    return [
      pr === undefined
        ? 'Pushed its branch.'
        : `Pushed its branch and updated pull request #${pr}.`,
    ]
  },
  merged: (f, line) => merged(f, line),
  'review-round-ended': (f) => [reviewRoundEnded(f)],
  'merge-refused': (f) => [`GitHub refused the merge: ${field(f, 'reason') ?? ''}.`],
  'closed-on-tracker': (f) => [closedOnTracker(f)],
  'close-on-tracker': said('Not closed on the tracker: no tracker is set.'),
  'started-on-tracker': said('Marked started on the tracker.'),
  'issue-begun': said('Started: the lead is reading the issue.'),
  decision: (f) => decision(f),
  'decision-answered': said('You answered in Slack.'),
  'decision-action': said('You answered in Slack.'),
  'decision-reply': said('You answered in Slack.'),
  'decision-superseded': said('A question in Slack no longer needs your answer.'),
  told: said('Told you in Slack what to know or prepare.'),
  reminded: (f) => [
    `Reminded you in Slack of ${numberField(f, 'waiting') ?? 'the'} decisions still waiting.`,
  ],
  paused: (f) => paused(f),
  resumed: (_, line) =>
    line.issue === null && line.sprint === null
      ? ['Resumed. You sent ', { code: '!resume' }, ' in Slack.']
      : ['Goes on: what stopped it was resolved.'],
  command: (f) => ['You sent ', { code: `!${field(f, 'command') ?? ''}` }, ' in Slack.'],
  'ignored-action': ignored,
  'ignored-reply': ignored,
  'ignored-command': ignored,
  'ignored-message': ignored,
  preview: said('Started the preview you asked for.'),
  'preview-refused': (f) => [`Could not start the preview: ${field(f, 'why') ?? ''}.`],
  'channel-error': failedWith('Slack did not accept a message: '),
  'config-reloaded': (f) => [settingsChanged(f)],
  'config-refused': failedWith('Settings not applied: '),
  'goal-set': said('You changed the product goal.'),
  prerequisites: (f) => {
    const missing = listed(f['missing'])
    return [missing === undefined ? 'Nothing is missing to work.' : `Missing to work: ${missing}.`]
  },
  'tracker-issues': said('The open issues on the tracker changed.'),
  'operation-failed': (f) => [
    `Could not finish ${field(f, 'operation') ?? 'an operation'}: ${field(f, 'error') ?? ''}.`,
  ],
  stopped: (f) => [
    `Stopped: ${field(f, 'operation') ?? 'an operation'} kept failing: ${field(f, 'error') ?? ''}.`,
  ],
  filed: (f) => [`Filed ${stepNamed(f)} for an agent.`],
  reconciled: (f) => [`Filed ${stepNamed(f)} again: it was missing.`],
  'relaunch-after-restart': said('Started an agent again after a restart.'),
  retry: (f) => [
    `An agent run failed for a moment and is tried again: ${field(f, 'reason') ?? ''}.`,
  ],
  'resume-failed': said('An agent could not go on where it was and starts over.'),
  failed: said('An agent ended without finishing its work.'),
  'unreadable-output': failedWith("An agent's output could not be read: "),
  'task-files-unreadable': failedWith("The plan's task files could not be read: "),
  'nothing-to-commit': said('Nothing to commit.'),
  committed: said('Committed its work.'),
  'commit-refused': failedWith('The repository refused the commit: '),
  'push-refused': failedWith('The push was refused: '),
  'push-conflict': said('The push met a conflict with the base branch.'),
  'push-failed': failedWith('Could not push: ', '. It is tried again.'),
  'review-requested': (f) => [
    `Asked ${listed(f['reviewers']) ?? 'its reviewers'} to review its pull request again.`,
  ],
  'head-conflict': said('Took in commits someone else pushed to its branch.'),
  'head-taken': said('Took in commits someone else pushed to its branch.'),
  'checks-failed': (f) => [`CI failed: ${listed(f['failed']) ?? 'a check'}.`],
  'comments-accepted': said('Took in new comments on its pull request.'),
  replied: said('Replied to a comment on its pull request.'),
  'reply-failed': failedWith('Could not reply to a comment on its pull request: '),
  gate: said('What its merge waits on changed.'),
  'agreements-gate': said('What its merge waits on changed.'),
  'pull-closed': (f) => {
    const pr = numberField(f, 'pr')
    return [pr === undefined ? 'Closed its pull request.' : `Closed pull request #${pr}.`]
  },
  'branch-deleted': said('Deleted its merged branch.'),
  'github-error': failedWith('GitHub refused a request: '),
  canceled: said('You canceled it.'),
  'app-stop-error': failedWith('Could not stop its app: '),
  'item-ended-with-issue': said('Stopped its agent work: the issue ended.'),
  'recovery-started': said('Started the recovery you approved.'),
  'recovery-ended': (f) => [
    f['succeeded'] === true
      ? 'The recovery you approved finished.'
      : 'The recovery you approved failed.',
  ],
  'sprint-begun': said('Began a sprint.'),
  'sprint-item-ended': said("A sprint step's work ended."),
  'sprint-fact': (f, line) => [
    sprintFact(f) ?? fallbackSentence(camelWords(field(f, 'fact') ?? line.kind)),
  ],
  fact: (f) => [issueFact(f)],
  'agreements-pr': (f) => {
    const pr = numberField(f, 'pr')
    return [
      pr === undefined
        ? "Opened the working agreements' pull request."
        : `Opened the working agreements' pull request #${pr}.`,
    ]
  },
  'agreements-error': failedWith(
    "The working agreements' pull request failed: ",
    '. It is tried again.'
  ),
  'run-commands': said('The lead found how the product runs.'),
}

/** How many of a thing, in words: "1 finding", "3 findings", "0 findings". */
function counted(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

/** A merge, with the pull request's number and base when the line records them. */
function merged(f: Fields, line: EventLine): Part[] {
  const pr = numberField(f, 'pr')
  const base = field(f, 'base')
  if (pr === undefined) {
    return [
      line.issue === null && line.sprint !== null
        ? "Merged the working agreements' pull request."
        : 'Merged its pull request.',
    ]
  }
  return base === undefined
    ? [`Merged pull request #${pr}.`]
    : [`Merged pull request #${pr} into `, { code: base }, '.']
}

function reviewRoundEnded(f: Fields): string {
  const round = numberField(f, 'round')
  const findings = counted(numberField(f, 'findings') ?? 0, 'finding', 'findings')
  return round === undefined
    ? `A review round ended with ${findings}.`
    : `Review round ${round} ended with ${findings}.`
}

/** The proposal's summary a posted sprint decision records: `{issues, purpose}`. */
function proposalOf(f: Fields): { issues: number; purpose: string } | undefined {
  const p = f['proposal']
  if (typeof p !== 'object' || p === null || Array.isArray(p)) {
    return undefined
  }
  const { issues, purpose } = p
  return typeof issues === 'number' && typeof purpose === 'string' ? { issues, purpose } : undefined
}

/**
 * A decision Slack accepted (`posted`), with a sprint proposal's summary when it records one. A
 * line without `posted` was written when the decision was made, before it was sent.
 */
function decision(f: Fields): Part[] {
  const kind = field(f, 'kind') ?? ''
  if (f['posted'] !== true) {
    return [kind === 'sprint' ? 'Prepared a sprint proposal.' : 'Prepared a decision for you.']
  }
  const proposal = kind === 'sprint' ? proposalOf(f) : undefined
  return proposal === undefined
    ? [DECISION[kind] ?? 'Asked you in Slack to decide.']
    : [
        `Proposed the sprint in Slack: ${counted(proposal.issues, 'issue', 'issues')} toward `,
        { quote: proposal.purpose },
      ]
}

/** The reason an owner's `!pause` without words records. */
const OWNER_PAUSE = 'paused by the owner'

/** A pause: the owner's `!pause` (with their words) when it names who sent it, else its reason. */
function paused(f: Fields): Part[] {
  const reason = field(f, 'reason')
  const sent: Part[] = ['Paused. You sent ', { code: '!pause' }, ' in Slack']
  if (reason === undefined || reason === OWNER_PAUSE) {
    return [...sent, '.']
  }
  return field(f, 'user') === undefined
    ? ['Paused: ', { quote: reason }, '.']
    : [...sent, ': ', { quote: reason }, '.']
}

function ignored(f: Fields): Part[] {
  return [`Ignored a Slack message from ${field(f, 'user') ?? 'someone'}, who is not an owner.`]
}

function closedOnTracker(f: Fields): string {
  const tracker = trackerOf(f)
  const state = field(f, 'state')
  if (tracker !== undefined && state === 'done') {
    return `Marked done on ${tracker}.`
  }
  return tracker !== undefined && state === 'canceled'
    ? `Canceled on ${tracker}.`
    : 'Closed on the tracker.'
}

/** What a recorded line says happened, as one sentence; its Error or Cleared word comes apart. */
export function eventSentence(line: EventLine): Part[] {
  return SENTENCE[line.kind]?.(line.fields, line) ?? [fallbackSentence(line.kind)]
}

/** An error going on, by its name: "Can't reach GitHub", "The issue filter fails". */
export function errorName(e: ErrorNow): string {
  const tracker = e.tracker === null ? 'the tracker' : TRACKER[e.tracker]
  switch (e.key) {
    case 'github-unreachable':
      return "Can't reach GitHub"
    case 'tracker-unreachable':
      return `Can't reach ${tracker}`
    case 'tracker-refused':
      return `${e.tracker === null ? 'The tracker' : tracker} refuses the connection`
    case 'cli-claude':
    case 'cli-codex':
    case 'cli-opencode':
      return `${cliOf(e.key, e.cli === null ? {} : { cli: e.cli }) ?? 'The agent CLI'} cannot start a run`
    case 'filter':
      return 'The issue filter fails'
  }
}

/** What the owner does while an error lasts. */
export function errorFix(e: ErrorNow): Part[] {
  switch (e.key) {
    case 'github-unreachable':
    case 'tracker-unreachable':
      return [`Nothing to do unless it lasts. The factory tries again${interval(e.key)}.`]
    case 'tracker-refused':
      return ["Check the tracker's key or token in ", { settings: 'Settings' }, '.']
    case 'cli-claude':
    case 'cli-codex':
    case 'cli-opencode':
      return ['Fix it in a terminal on this computer, as the error says.']
    case 'filter':
      return [
        'Issues go on as without a filter until it works. Fix ',
        ...(e.command === null ? ['the filter command'] : [{ code: e.command }]),
        ' or change it in ',
        { settings: 'Settings' },
        '.',
      ]
  }
}

/** What a drop's link opens, named after its address: "Linear issue". */
export function trackerLinkName(url: string): 'Linear issue' | 'GitHub issue' | 'Tracker issue' {
  const host = URL.canParse(url) ? new URL(url).hostname : ''
  if (host === 'linear.app' || host.endsWith('.linear.app')) {
    return 'Linear issue'
  }
  return host === 'github.com' ? 'GitHub issue' : 'Tracker issue'
}
