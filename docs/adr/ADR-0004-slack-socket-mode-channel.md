# ADR-0004 Reach the owner through Slack Socket Mode behind a channel interface

## Status

Accepted

`Accepted` records the currently selected means, not an obligation to retain it. When later evidence supports a smaller sufficient choice, update or supersede this decision while the outcome, desired-future requirements, and non-goals remain true.

## Context

Every owner decision arrives and is answered in Slack, including approvals given with buttons whose presser's identity counts (AC-029 to AC-034). The Slack part must sit behind an interface that another channel can implement (AC-035). The factory runs on a machine that usually has no public address.

## Decision Point

- **Question**: How does the factory exchange messages and button actions with Slack?
- **Why a decision exists**: Socket Mode (a WebSocket the app opens), the Events API with a public request URL, and polling the channel's history all work for messages; they differ in whether interactive buttons are possible and whether a public endpoint is needed.
- **Scope boundary**: AC-029 to AC-035.

## Decision

### Decision Details

| Item | Content |
|------|---------|
| **Decision** | Connect to Slack with Socket Mode using an app-level token, receive messages, mentions and `block_actions` over it, post with the Web API, and put all of it behind a `Channel` trait whose operations are the factory's (post a decision, post in a thread, receive a reply, receive an action, receive a command), not Slack's. |
| **Why this** | Buttons and their presser's identity without a public endpoint; events arrive as they happen. |
| **Known unknowns** | Settled: Socket Mode is implemented directly on a WebSocket client; a dropped connection reopens with back-off and catches up from the channel's history. |
| **Reconsider when** | The factory runs where a public endpoint exists and Socket Mode's limits (ten connections, not listed in the Slack Marketplace) matter. |

## Rationale

### Options Considered

| Option | Product value from the requirements | Repository fit | Total complexity | Maintainability | Material trade-offs | Reversibility |
|--------|-------------------------|----------------|------------------|-----------------|---------------------|---------------|
| Socket Mode | All of AC-029 to AC-035 | New | WebSocket lifecycle and acknowledgements within 3 s | Behind the trait | App cannot be in the public Marketplace; app-level token to store | Replace behind the trait |
| Events API with a public URL | All | New | A tunnel or public host per factory | Behind the trait | Exposes an endpoint; owner must run a tunnel | Replace behind the trait |
| Polling history | Messages only; no buttons | New | Simple | Behind the trait | Approvals by typed reply only, identity from the message author; delay of one poll | Replace behind the trait |

**Selected**: Socket Mode.

## Consequences

### Positive Consequences

- Approvals carry the identity Slack asserts for the presser, recorded with the message link (AC-032).

### Negative Consequences

- The owner creates a Slack app with Socket Mode enabled from the manifest the dashboard's setup shows, and stores two tokens.

### Neutral Consequences

- Messages that arrive while the factory is stopped are read from the channel history on reconnect.

## Architecture Impact

Adds a `channel` module with the `Channel` trait and a Slack implementation; the factory core depends only on the trait.

## Related Information

- Slack Socket Mode: https://docs.slack.dev/apis/events-api/using-socket-mode/
- `apps.connections.open`: https://docs.slack.dev/reference/methods/apps.connections.open
