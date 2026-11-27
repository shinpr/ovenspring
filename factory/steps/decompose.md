# Step: decompose

## Required skills

Load and read each skill completely before acting on your item: `ovenspring-ai-development-guide`, `ovenspring-documentation-criteria`, `ovenspring-testing-principles`, `ovenspring-coding-principles`, `ovenspring-implementation-approach`, `ovenspring-llm-friendly-context`.

## In the factory

- Write to the path in your item's `out` field: `{"status": "completed", "paths": ["<each task file>"]}`.

You convert a Work Plan into executable task files while preserving its task boundaries and implementation scope.

## Execution Gate

Before acting, map the preloaded skills to concrete rules for this task. Follow the applicable process below, advancing only when the current step's required evidence is present. Before finishing, verify that your work satisfies those rules and the requirements below.

## Input

- Exact Work Plan path

## Responsibility

Task decomposition is a mechanical handoff. Each generated task maps to exactly one Work Plan task ID and preserves its outcome, sources, scope, dependencies, and verification. New requirements, design decisions, technical reinterpretations, operating procedures, and external preparation are outside this transformation.

## Process

### 1. Read the task set

Extract each Work Plan task's:

- task ID and implementation outcome;
- cited Design Doc, ADR, or UI Spec sections and AC IDs;
- target responsibility or expected files;
- dependencies;
- verification method;
- optional Primary failure and Observable check.

### 2. Preserve task boundaries

Generate exactly one implementation task file per Work Plan task. Copy dependency task IDs unchanged. Assign `NN` as the zero-padded ordinal of the task's appearance in the Work Plan. Use `{plan-name}-task-{NN}.md`. Execution order comes from dependency task IDs, not filenames.

### 3. Resolve implementation context

For each task:

1. Copy every governing-source citation unchanged into `Governing Sources`.
2. Add those cited sections, the target implementation, and one representative adjacent test to `Investigation Targets`.
3. When an exact file is not yet knowable, name the smallest owner directory or module and the search criterion the executor can resolve.

The task file points to authoritative governing content instead of reproducing it. The executor reads every Investigation Target before implementation.

Preserve the path of an existing generated test skeleton named by the Work Plan, and its completion, in the task outcome and completion criteria.

### 4. Preserve verification intent

Create Operation Verification Methods from the Work Plan task's verification and cited governing sections. Keep exact contracts and protected boundaries authoritative in their cited sources and verify their observable effect.

Copy `Verification Focus` unchanged when the Work Plan provides it. Otherwise use the task's normal verification.

Tests, repository configuration, fixtures, migrations, mocks, wiring, and documentation stay in the implementation task that makes them complete unless the Work Plan defines an independent repository deliverable.

### 5. Generate task files

Use `references/task-template.md` in the ovenspring-documentation-criteria skill and write files under the path your request names.

Each task contains:

- Source Work Plan Task;
- Implementation Outcome;
- Governing Sources;
- Investigation Targets;
- Operation Verification Methods;
- optional Verification Focus copied from the Work Plan;
- Completion Criteria tied to cited ACs.

## Self-Validation [BLOCKING — before writing the output]

Complete every item before writing the output. When an item is unsatisfied, go back to the relevant decomposition step.

- [ ] Every generated task maps to exactly one Work Plan task ID.
- [ ] Every source citation is preserved unchanged.
- [ ] Every source task appears exactly once.
- [ ] Generated outcomes are subsets of the Work Plan's outcomes.
- [ ] Dependencies and test skeleton paths are copied unchanged.
- [ ] Target and investigation context is concrete enough for the executor to start.
- [ ] No governing technical content is copied or reinterpreted in the task file.
- [ ] Every task produces a repository implementation outcome.
