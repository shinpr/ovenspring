# GitHub account for factory Git operations

## Overview

Factory-created commits use the configured GitHub account in both Git identity fields. Factory publication and its remote reads use the same token as the GitHub API. The daemon applies these overrides to its own Git commands so the owner's Git setup and manual workflow are preserved.

## Background and Context

### Prerequisite ADRs

[ADR-0007](../adr/ADR-0007-daemon-owns-external-writes.md#decision) assigns GitHub credentials and external writes to the daemon. [ADR-0006](../adr/ADR-0006-agent-launch-permissions-and-guard.md#decision) governs agent launches. The configured credential goes to the factory's Git transport. Agent launches and product commands retain their current environment and guard.

### External Resources Used

| Resource | Feature-specific identifier | Use |
|---|---|---|
| GitHub API and Git transport, configured through Connect in [GUIDE.md](../GUIDE.md#connect) | REST `GET /user` for account ID and login. Smart HTTPS Git on the connected repository. | Resolve the configured account's Git identity and authenticate publication and its reads. |

### Requirement Convergence

- **Outcome** — Issue and agreements publication is attributed to the configured token's account in both commit roles and in the push.
- **Requirements** — Cover all factory-created branch commits, including local merges. Use the existing token even when the machine has no GitHub credential helper login. Preserve the owner's Git settings and stored credentials. Update the commit identity rule in [Factory Core Design](factory-core-design.md).
- **Non-goals** — Existing pushed commits, manual commits, deployment behavior and deployment checks. The owner's answer in `state/sources/answers/GH-34-spec-19.md` also excludes pushes to norsica.
- **Owner decision** — Derive the ID-based noreply address from authenticated `GET /user` with the current grants and Connect inputs. A legacy account may lack a GitHub profile link. See `state/sources/answers/GH-34-spec-20.md` (Answer).

Scope and scale come from `state/outputs/20261128-055634-requirements-GH-34.json` (convergence, Medium) and `state/sources/issues/GH-34.md` (Requirements and Done when).

### Standards and Assumptions

| Standard or premise | Evidence and design effect | Confirmed |
|---|---|---|
| Factory owns the write and preserves host configuration | ADR-0007 and GH-34 Requirements. Existing `worktree::git` uses a child process. | Yes |
| Git identity overrides set both commit roles, including a real merge | [Git identity variables](https://git-scm.com/docs/git#_git_commits) and the two-parent merge observation in codebase analysis `decisionMaterials.reuse`. | Yes |
| An explicit fetch refspec updates its named destination ref. Git can apply both rewrite families to a literal URL. | [Git fetch](https://git-scm.com/docs/git-fetch#_configured_remote_tracking_branches), [Git URL configuration](https://git-scm.com/docs/git-config#Documentation/git-config.txt-urlltbasegtinsteadOf) and [Git URL resolution](https://github.com/git/git/blob/v2.54.0/remote.c). The transport supplies each tracking destination and guards the literal URL before Git uses it. | Yes |
| GitHub accepts a token as the HTTPS Git password | [Token authentication](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#using-a-personal-access-token-on-the-command-line). | Yes |
| Authenticated `GET /user` supplies ID and login with the existing Connect grants | GitHub documents both fields in the public user response returned to classic and OAuth tokens without private-user scopes. Fine-grained PATs need no permissions for this endpoint. The address uses those fields even when the profile email is absent. [GitHub REST response and permissions](https://docs.github.com/en/rest/users/users#get-the-authenticated-user) | Yes |

Adopt the Rust checks in [CI](../../.github/workflows/ci.yml) for the touched paths. Use the existing Ubuntu and macOS offline lane with workspace tests, formatting, Clippy, release build and dependency checks.

## Acceptance Criteria

- **AC1** — When the factory creates an issue commit, a local integration merge or an agreements commit, both roles in `git log --format='%an <%ae> | %cn <%ce>'` shall show the configured token account's login and `{id}+{login}@users.noreply.github.com`.
- **AC2** — When the factory pushes an issue or agreements branch with no GitHub login available through the owner's Git helpers, the remote shall receive the branch using the configured token.
- **AC3** — When the factory commits or publishes, the owner's Git configuration and stored credentials shall retain their values and previously published commit objects shall retain their identity.

## Existing Codebase Analysis

The affected path begins at `crates/cli/src/daemon_cmd.rs::services_for`. It builds `GitHubApi`, which serves pull requests and GitHub Issues using one selected token. The daemon stages and creates commits in `issues::commit` and `sprint::open_agreements_pr`, integrates outside commits through `pulls::merge_from_origin`, and pushes from `pulls::push` and `sprint::push_agreements`. The final merge still goes through the existing checked-head API call.

### Fact Disposition Table

| Fact ID | Focus Area | Disposition | Rationale | Evidence |
|---|---|---|---|---|
| `crates/cli/src/daemon_cmd.rs:services_for` | One token source and service lifetime | preserve | Keep explicit token before `gh` and the service replacement boundary. Add identity only at a commit operation. | crates/cli/src/daemon_cmd.rs:225,311,329; crates/daemon/src/lib.rs:317; crates/integrations/src/github.rs:222,250,345,575; crates/cli/src/setup_checks.rs:91; crates/cli/tests/support/fake_github.rs:289 |
| `crates/daemon/src/worktree.rs:git` | Local commit creation and inherited identity | transform | Give both identity roles to the commit-producing invocations. Keep empty staging, ancestor skips, fast-forwards, conflicts and retries. | crates/daemon/src/worktree.rs:15; crates/daemon/src/issues.rs:1096,1324; crates/daemon/src/pulls.rs:284,906; crates/daemon/src/lib.rs:613; crates/daemon/src/sprint.rs:479,511; docs/design/factory-core-design.md:476 |
| `crates/cli/src/setup_checks.rs:repo_id` | Git destination and tracking refs | transform | Use the API-bound repository over HTTPS. Update the tracking refs used by the factory through explicit fetch refspecs. | crates/cli/src/setup_checks.rs:25; crates/integrations/src/github.rs:20; docs/GUIDE.md:47; crates/daemon/src/pulls.rs:186,284,706,906; crates/daemon/src/sprint.rs:730; crates/daemon/src/worktree.rs:84 |
| `crates/daemon/src/pulls.rs:push` | Publication reachability, remote reads and recovery | transform | Use the same authenticated Git connection for these reads. Separate absent branch, refused write, unavailable network and unusable connection. | crates/daemon/src/pulls.rs:186,284,327,459,706; crates/daemon/src/stops.rs:73,157; crates/core/src/flow.rs:1067; crates/daemon/src/lib.rs:601; crates/server/src/setup.rs:431 |
| `crates/daemon/src/records.rs:append_event` | Credential lifetime and error exposure | preserve | Keep the event, refusal and stop contracts. Sanitize transport output where the credentials are applied. | crates/daemon/src/worktree.rs:15; crates/daemon/src/issues.rs:1141; crates/daemon/src/pulls.rs:210; crates/daemon/src/stops.rs:93; crates/daemon/src/records.rs:180; docs/adr/ADR-0007-daemon-owns-external-writes.md:25 |
| `crates/integrations/src/github.rs:merge` | GitHub-generated final merge versus local branch merges | preserve | GitHub supplies final merge metadata. The existing guard, expected head SHA and allowed repository merge method still apply. | crates/integrations/src/github.rs:503; crates/daemon/src/pulls.rs:772; docs/prd/oven-spring-prd.md:169; GH-34.md, Current state and Done when |

## Design

### Selected Design

Use the existing `GitHubApi` instance for authenticated Git access and token-account lookup. Factor the duplicated token selection in `cli::setup_checks` and `cli::daemon_cmd` into one function that preserves explicit-token precedence, the `gh` fallback and its masking value. The daemon uses the client in its current `Services` snapshot, so replacing services also replaces the account and credential used by subsequent operations. Setup and Settings use the same selector against their saved connection. There is no separate credential, serialized state or identity cache.

**Identity boundary.** Add a commit identity operation on the current `GitHub` service. In `GitHubApi`, reuse its authenticated REST `GET /user` request path and decode the account's `id` and `login`. Set both names to `login` and derive both email values as `{id}+{login}@users.noreply.github.com`. Keep the existing `viewer()` login result and the response handling for review and comment authors. Identity lookup failure follows the connection failure path before Git runs. The operation uses the current Connect grants and inputs.

**Git commands.** Give `GitHub` an internal Git origin capability with `fetch`, `head_exists` and `push_branch`. Implement the production capability on the existing `GitHubApi` so the token stays private to the client and its Git adapter. Move the current process executor to a Git module in integrations, re-export its plain operation through `worktree`, and extend the executor for identity-scoped local commands and authenticated remote commands. Identity and token settings apply to separate commands. Keep the hooks that the owner configured for Git, including pre-push, and treat them as part of the trusted local Git process tree.

**Destination.** Build `https://github.com/<owner>/<name>.git` from the `RepoId` already bound to the API client. Keep the existing origin-to-`RepoId` resolution for Connect, including SSH-form origins. Validate the owner and repository as single GitHub identifier path segments before formatting the URL. Fetches, probes and pushes pass this URL directly. Factory fetches set each destination ref explicitly. Git still runs the configured pre-push hook. Git supplies the URL as both hook arguments for an unnamed remote. The repository's `lefthook.yml` and the refusal hooks in `crates/cli/tests/issue_flow_int.rs` exercise pre-push checks without requiring the name `origin`. [Git hook arguments](https://git-scm.com/docs/githooks#_pre_push)

Before launching a network command, read URL rewrite keys with `git config --includes --null --get-regexp` in the same worktree and sanitized Git environment. Select `^url\..*\.insteadof$` for reads and `^url\..*\.(insteadof|pushinsteadof)$` for pushes. Git emits a key and value separated by a newline in each NUL-terminated record. Treat exit 1 with no output as an empty set and propagate other failures. Reject any selected rule whose value is a prefix of the canonical URL. An operation failure names the connected repository and the conflicting config key type, without printing config values. Restrict the child to HTTPS with TLS verification, redirects disabled and submodule recursion disabled. The authorization header is scoped to the canonical endpoint.

**Authentication.** Supply a URL-scoped Basic header in process configuration, clearing the effective inherited extra-header list first. Disable inherited credential helpers, askpass, terminal prompts and Git tracing in the authenticated child. Pass the raw or encoded token in neither command arguments nor URLs. Before returning a diagnostic, redact the token and its encoded Basic value from stdout and stderr. Error records retain the operation, safe repository identifier and a sanitized cause. Local commit and merge commands receive only the identity overrides. [Git HTTP options and runtime configuration](https://git-scm.com/docs/git-config#_environment), [Git credential helpers](https://git-scm.com/docs/gitcredentials#_configuration_options)

| Added surface | Current evidence and lower-surface insufficiency | Subtraction result |
|---|---|---|
| Commit identity operation and process-scoped identity | The current viewer exposes only the login and serves Connect and reachability checks. Reuse its authenticated request and keep the stricter decode at the commit identity boundary. The Git runner inherits machine identity. | Removing the operation loses the account ID for the address. Removing the child overrides leaves machine identity in new commits. GH-34 Requirements and AC1. |
| Scoped HTTPS Git execution and explicit fetch destinations | The existing API binding supplies one repository. Git needs the scoped token and a direct HTTPS URL to reach that repository from an SSH checkout, a rewrite guard to enforce the destination, and explicit refspecs to update the refs consumed by merges, gate reads and detached checkout. | Removing credential scope breaks isolation or publication without a helper login. Removing explicit refspecs leaves those consumers with stale tracking refs. GH-34 Requirements, AC2 and AC3. |

### Integration and State Transitions

| Existing consumer | Revised invocation and preserved behavior |
|---|---|
| `issues::commit`, including all flow commit points | Keep staging and empty-index handling. Resolve identity just before a nonempty commit. Keep the command and its hook refusal behavior and record the resulting HEAD. A failure to acquire identity propagates before the commit refusal handler. |
| `sprint::open_agreements_pr` | Resolve identity only for a staged change that needs a commit. A retry of PR creation reuses the commit. Update the superseded identity comments in this route and in `pulls::bring_in`. The corresponding commit identity rule is in the Sprint states passage of [Factory Core Design](factory-core-design.md#commits). |
| `pulls::merge_from_origin`, reached by both publication routes and `Daemon::bring_in_pushed` | Fetch with the connection and keep last ref first. Keep ancestor skips. When an incoming ref needs integration, resolve the identity before invoking the existing merge command so Git can apply the repository's fast-forward policy. A fast-forward creates no new commit. On conflict, abort the merge and return the conflict to the existing caller. |
| `pulls::bring_in`, issue push, and agreements first push and repush | Probe the branch with an exact full ref, fetch and integrate as before, then call the same `push_branch` with `HEAD:refs/heads/<branch>`. A successful issue push still records the actual pushed HEAD, opens or updates the PR and re-requests reviewers through the current flow. A successful agreements push keeps its existing PR and gate behavior. |
| `pulls::observe_pull` outside-head path, `gate_view` and `Daemon::checkout_for` | Every publication-dependent fetch uses the connection. Gate fetches both refs and uses the same origin tracking refs and checked head. The detached checkout uses the authenticated base fetch. |
| `stops::origin_answers`, used by `stops::unreachable` and `pulls::poll_pull_requests`, and issue push refusal handling | Use the origin capability's `head_exists` probe with `HEAD` at the connected HTTPS endpoint. The [Events design](events-and-error-notices-design.md#errors) owns incident start, repeat and clear. Issue push distinguishes a refused write from unreachability using this same endpoint. |
| `server::setup::settle_app` and Settings' call to it | Pass the same saved token source through the existing `SetupChecks` backend seam when refreshing the detached base for **Run the product**. The CLI implementation uses the configured Git origin capability. Keep the existing detached checkout and Setup error response. |

Keep durable flow state and persisted credentials in their current formats. Git updates the existing origin tracking refs. The settings and setup JSON shapes, agent prompts and GitHub's final merge request stay as shipped. A credential replacement affects subsequent operations and leaves existing commit objects intact.

### Data Contracts and Field Propagation

| Field and boundary | Serialized representation | Consumer and validation |
|---|---|---|
| GitHub account to commit identity | Authenticated `GET /user` with the existing Bearer token and no request body. Read JSON `id` as a positive unsigned integer and `login` as a nonempty string. | Require both fields for this operation. Reject whitespace, control characters and `<`, `>` or `@` in the login so the value reaches Git verbatim. Ignore other response fields. A failed request or unusable response follows the connection failure path. |
| Identity to commit or merge | Child environment `GIT_AUTHOR_NAME` and `GIT_COMMITTER_NAME` both set to the login. `GIT_AUTHOR_EMAIL` and `GIT_COMMITTER_EMAIL` both set to `{id}+{login}@users.noreply.github.com` with the ID in decimal. | Git writes the author and committer lines in each new commit object. When the operation makes no commit, it writes no identity. |
| Destination and authorization to remote Git | `GIT_CONFIG_COUNT` decimal count and numbered `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>` pairs starting at zero. Pass the canonical URL as the repository argument. Set `http.<canonical>.extraHeader` to empty and then to `Authorization: Basic <base64(x-access-token:token)>`. Encode the UTF-8 credentials with standard padded Base64. Set matching `http.<canonical>.sslVerify=true` and `http.<canonical>.followRedirects=false`. | Git parses process config without persisting it. Use empty `credential.helper`, empty `core.askPass`, `GIT_TERMINAL_PROMPT=0`, clear inherited `GIT_ASKPASS`, `SSH_ASKPASS`, `GIT_SSL_NO_VERIFY`, trace variables and inherited command-scope Git overrides before setting factory values. Use `GIT_ALLOW_PROTOCOL=https` for effective protocol verification. Reset inherited extra headers before supplying the sole authorization header. |
| Remote branch and fetched objects | `ls-remote --exit-code <canonical> <ref>` takes `refs/heads/<branch>` for branch presence or `HEAD` for reachability. `fetch -q --no-recurse-submodules --refmap= <canonical> +refs/heads/<ref>:refs/remotes/origin/<ref> ...` takes one explicit mapping per requested branch. `push -q --recurse-submodules=no <canonical> HEAD:refs/heads/<branch>` publishes the selected branch. | Match the requested ref in Git's response. Exit 2 means the endpoint answered without that ref. It means an absent branch for branch presence and counts as an answer for reachability. Other exits use the error path. Fetch updates tracking refs even when an upstream branch moves backward. Push updates the selected branch without force. |

### Error Handling

| Boundary | Failure | Existing recovery owner |
|---|---|---|
| Identity, endpoint or write credentials | Missing connection, invalid destination, unusable identity, Git authentication failure, or an explicit transport authorization denial | Route through the issue or sprint operation failure and existing stop/retry flow. Give actionable context for the failed GitHub Connect lookup, repository token access or endpoint configuration, as applicable. API unavailability uses existing API recovery. |
| Git remote reads and probes | Branch absence, network failure or an unusable credential | Recognize Git's HTTP authentication and authorization diagnostics under a fixed C locale before the reachability fallback. An explicit denial is an operation failure on the issuing operation and proves the endpoint answered for an open reachability incident. A transport failure participates in the existing incident and retry handling. Local endpoint configuration errors follow the operation failure path. A probe that fails locally supplies no reachability result. |
| Publication | Hook or repository refuses a write and the same endpoint answers the probe with successful authentication | Preserve issue `PushRefused` and its quality correction with sanitized diagnostics. If the push or its probe reports a recognized credential denial or local endpoint configuration failure, use operation failure handling. Unreachable endpoints use existing retry behavior. Agreements keep their current sprint failure handling. Read and record local HEAD after push success separately from classifying a push error. |
| Local Git | Hook refusal or merge conflict after identity was resolved | Preserve commit refusal and merge-abort behavior. Preserve a merge conflict only when Git left unresolved entries. A non-conflict merge failure with no unresolved entries propagates as a Git operation failure. |

## Implementation Plan

Use a vertical slice through the existing issue path. Begin with the shared account identity and authenticated Git boundaries. Verify the authenticated origin and tracking-ref behavior at the real Git subprocess boundary before wiring every caller. Next route the issue commit, merge and push through those capabilities and exercise rejected-push recovery. Move the agreements and dependent reads onto the same implementation, including the setup checkout. Run the affected check lanes with the updated factory-core and Events contracts. The existing branch, records and credentials need no migration.

## Security Considerations

Construct the destination from the connected repository and guard against applicable URL rewrites before handing Git a credential. Use the same nonsecret command-scope settings for rewrite inspection and execution. The transport enforces TLS and disables redirects so the scoped header can only be sent to the selected GitHub endpoint. Keep configured owner Git hooks in the trusted child process boundary. Never return secret values in adapter diagnostics, include them in debug formatting, place them in Git settings on disk, or pass them into factory-launched agents or product checks. Use the smallest existing token permissions that satisfy the connection. Leave public API and setup serialization unchanged.

## Test Boundaries and Verification Strategy

| Proof | Repository-controlled boundary |
|---|---|
| Identity source and mapping | Extend `crates/integrations/tests/github_contract.rs` to verify authenticated `GET /user` and the exact mapping from `{"id":12345678,"login":"factory-bot","email":null}` to `factory-bot <12345678+factory-bot@users.noreply.github.com>`. Keep the existing login-only viewer fixture and exercise an unusable account response at the connection failure boundary before Git creates a commit. |
| Earliest implementation proof, AC2 and AC3 | Run the production Git transport adapter against an offline Smart HTTPS fixture backed by `git http-backend` and a temporary bare repo. Terminate HTTPS with a test CA and a test-injected endpoint. Start from an SSH-form origin. Require the test token for ref advertisement, upload-pack and receive-pack. Assert the `HEAD` probe receives the authenticated ref advertisement, the fetch puts the advertised OID in `refs/remotes/origin/<ref>` and the push puts the sent SHA in the remote branch. Seed an inherited header and credential helper, assert the helper was never called and compare Git config bytes. Check that matching fetch and push rewrite rules fail before a network request. If authentication, trust or the refmap fails, revisit the transport design before routing remaining callers. |
| New commits and preserved objects, AC1 and AC3 | Extend the existing issue, outside-change and sprint integration fixtures. Assert both author and committer names and email addresses on an issue commit, actual two-parent merges before publication and pre-launch, and the agreements commit. Keep a distinct ambient owner identity. Inspect the resulting Git objects and assert existing and outside commits retain their metadata. In `sprint_int.rs`, replace the old author-only repository-identity expectation and check downstream failure reuses the existing commit. |
| Recovery and route wiring | Drive issue push and correction and agreements publication through their existing daemon entry points, observing calls to the same origin capability. Use the existing pre-push hook refusal tests in `issue_flow_int.rs` and outage and recovery cases in `resilience_int.rs`, changing reachability at the fake origin capability. Exercise an invalid test credential against the authenticated adapter and check that the issuing flow records a sanitized operation failure, with no push refusal correction or new reachability incident. Observe the detached checkout at the existing Setup backend seam. Keep gate-level tests for base and head checks. |

The transport fixture substitutes the external GitHub endpoint only. Real Git execution and ref updates own proof of authentication and configuration isolation. Daemon flow fixtures can continue to use the bare repo through a fake origin capability, with a distinct identity from the repository's Git config. The authenticated transport fixture owns token acceptance evidence. Local commit inspection provides the acceptance evidence for account metadata.

**Output comparison.** Use identical `acme/storefront` repository topology, local changes and token fixture across the affected boundaries. The expected origin is the same `acme/storefront` API repo and the factory transport uses its HTTPS endpoint. A present owner-selected token reaches both the API as Bearer and Git as the test's Basic value. The absent-token case selects the existing `gh` token at that same boundary. For the representative account response, both roles on new commits change from the ambient owner identity to `factory-bot <12345678+factory-bot@users.noreply.github.com>`. A divergent base and branch must supply the same fetched OIDs and parents via `refs/remotes/origin/<ref>`. The newly created commit hash may differ. Compare the tree, ordered parents, ref destinations, both identity fields and pre-existing commit objects separately. Validate the advertised SHA, pushed ref and saved config using the fixture and Git inspection, then run the touched integration targets and the workspace CI gate.

## Design Boundaries

The gate retains GitHub's final merge, server-side rules and checked head SHA. The branch identity rule covers commits produced by local factory Git. Authoring already on origin and manual owner work are outside that rule. GitHub may omit the profile link for accounts using the legacy noreply format. Those commits still record the login and derived address, and publication proceeds under the configured token. The design adds authenticated Smart HTTPS use to the existing GitHub connection, which requires the `external-connection` design approval. [GitHub noreply formats](https://docs.github.com/en/account-and-profile/reference/email-addresses-reference#your-noreply-email-address)

## References

- `state/outputs/20261128-055833-codebase-analysis-GH-34.json`, especially `focusAreas`, `decisionMaterials` and `dataTransformationPipelines`.
- [Factory Core Design](factory-core-design.md), Pull request, State transitions and Error handling.
- [Product requirements](../prd/oven-spring-prd.md), AC-026, AC-028 and AC-029.
