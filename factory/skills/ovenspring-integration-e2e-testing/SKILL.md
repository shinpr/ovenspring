---
name: ovenspring-integration-e2e-testing
description: Defines integration/E2E proof boundaries and file conventions, and route-parity review criteria. Use when generating or consuming test skeletons or reviewing shared mutation routes.
---

# Integration and E2E Skeleton Contracts

## Test Lanes

| Lane | Observable boundary | Dependencies |
|------|---------------------|--------------|
| `integration` | Interactions within a process, including framework-rendered UI and module/API handlers | Real participating components; mock only outside the claimed boundary |
| `fixture-e2e` | Actual UI behavior in the real UI runtime (browser, simulator, emulator, or device) with deterministic fixtures | Fixtures may replace dependencies outside the claimed boundary |
| `service-integration-e2e` | Outcome depends on actual cross-service interaction or persistence | Required local services and stores are real; stubs only outside that proof boundary |

## Test File Naming Convention

Name test files by the repository's existing convention and the project runner's discovery rule. When the repository has no lane-separating convention and its runner discovers `*.test.*` files, use:

- Integration tests: `*.int.test.*` or `*.integration.test.*`
- fixture-e2e tests: `*.fixture.e2e.test.*` (or organize under `tests/e2e/fixture/`)
- service-integration-e2e tests: `*.service.e2e.test.*` (or organize under `tests/e2e/service/`)

## Route Parity for Shared Mutations

When multiple routes reach the same mutation, compare validation, classification, resource bounds, and read/parse/mutation/reporting order. A difference becomes a proof obligation when it can violate an authoritative contract or expose a material bypass in the changed boundary. Reuse existing proof or add the smallest test that detects that failure.
