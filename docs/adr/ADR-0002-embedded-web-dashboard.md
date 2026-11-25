# ADR-0002 Serve the dashboard as a React application embedded in the binary

## Status

Accepted

`Accepted` records the currently selected means, not an obligation to retain it. When later evidence supports a smaller sufficient choice, update or supersede this decision while the outcome, desired-future requirements, and non-goals remain true.

## Context

The dashboard shows the goal, the factory's state, settings and logs (PRD R9). It must meet WCAG 2.2 AA in light and dark themes (AC-048), follow the approved UI Spec and its prototype, and keep its colours in design tokens so that contrast pairs can be checked, with components developed in Storybook. The factory may run on a machine without a display, such as a dedicated host reached over SSH.

## Decision Point

- **Question**: What technology renders the dashboard?
- **Why a decision exists**: A native window (GPUI), server-rendered HTML from Rust templates, and a client-side React application are all credible and differ in where the UI can be viewed, accessibility, and tooling.
- **Scope boundary**: AC-043 to AC-050, AC-055, AC-056; the UI Spec `docs/ui-spec/oven-spring-ui-spec.md`.

## Decision

### Decision Details

| Item | Content |
|------|---------|
| **Decision** | Build the dashboard as a React and TypeScript application with Vite, develop its components in Storybook, embed the built assets in the binary, and serve them with a JSON API from the daemon on 127.0.0.1. |
| **Why this** | Viewable from any browser, including through an SSH port forward; accessibility comes from the browser; Storybook and tokens are native to the stack; the prototype maps directly. |
| **Known unknowns** | None that change the choice. |
| **Reconsider when** | The owner wants a desktop presence (menu bar, notifications) on the same machine; a native client can then be added against the same API. |

## Rationale

### Options Considered

| Option | Product value from the requirements | Repository fit | Total complexity | Maintainability | Material trade-offs | Reversibility |
|--------|-------------------------|----------------|------------------|-----------------|---------------------|---------------|
| React SPA embedded | All of R9, Storybook, AC-048 through the browser | Prototype is HTML/CSS | Node toolchain at development time; one JSON API | Mainstream stack agents write well | Local HTTP surface to protect (AC-055) | Replace the UI against the same API |
| Server-rendered HTML (Rust templates) | R9 and AC-048 | Prototype is HTML/CSS | No Node; interactivity by hand | Templates without component isolation | No Storybook; expanding rows, forms and live status need custom script | Moderate |
| GPUI native window | R9 on the same machine only | None | No HTTP surface | Pre-stable API, several forks | Not viewable on a headless host; screen reader support unclear upstream; no Storybook | Rewrite |

**Selected**: React SPA embedded.

## Consequences

### Positive Consequences

- The UI Spec's golden states become Storybook stories and serve as visual acceptance.
- Tokens are defined once in DTCG format and generated to CSS variables.

### Negative Consequences

- The daemon exposes a local HTTP server that can change settings; it binds to 127.0.0.1 and answers only requests naming this machine, with JSON bodies for writes (AC-055). These checks stop another site acting through the owner's browser without a token, which would not stop an agent running as the owner.

### Neutral Consequences

- Fonts are bundled with the assets; the dashboard makes no request outside the machine.

## Architecture Impact

Adds a `dashboard/` frontend package (React, TypeScript, Vite, Storybook) built into the binary at compile time, and an HTTP module in the daemon serving the assets and a versioned JSON API.

## Related Information

- UI Spec: `docs/ui-spec/oven-spring-ui-spec.md`
- ADR-0001
