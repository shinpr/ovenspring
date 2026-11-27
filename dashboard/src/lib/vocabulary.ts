// The words the dashboard shows for each code the API returns. The glance layer (Now) uses short
// fixed words; the read layer (Issue, Activity) uses sentences that name who acts.

import type {
  Activity,
  Ask,
  Cause,
  Cli,
  DocumentKind,
  Ending,
  LaunchRow,
  Phase,
  Position,
  Preparation,
  Role,
  RunResult,
  Scale,
  Step,
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
