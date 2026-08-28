# ADR-0019: Verification-first development against live infrastructure

- **Status:** Accepted
- **Date:** 2026-07-19 *(recording the methodology the project has followed since Phase 0)*
- **Deciders:** Project author

## Context

The project makes strong claims — "a published version is immutable", "tenants cannot
see each other", "the audit log cannot be tampered with by the app", "replaying a write
is safe". Claims of this kind fail in the seams *between* layers (ORM ↔ SQL grants,
serde ↔ constructors, pool ↔ session state), which is precisely where unit tests with
mocks cannot see. Meanwhile a solo-developer project with an AI pair-programmer
(see [docs/project-log.md](../archive/project-log.md)) has a specific risk profile: code is
produced quickly, so the binding constraint is not writing it but *knowing it is true*.

## Decision

Every feature ships with an **executable verification script** that talks to the real
server and the real database — no mocks — and the collection is runnable as one gate
(`scripts/all-check.sh`, currently 16 suites, ~1,000 assertions). Three rules give the
scripts their teeth:

1. **Negative space is asserted as deliberately as happy paths.** What must be
   *refused* — a draft assigned, a second tenant's rows, an UPDATE on performed sets, a
   reused invitation — is asserted at the layer that refuses it, including raw psql
   against triggers and grants (with `ON_ERROR_STOP`, without which negative SQL
   assertions pass vacuously — itself a lesson a script now pins).
2. **Pure logic is extracted and tested without infrastructure.** Navigation manifests,
   date bucketing, progress math and timer arithmetic are pure modules with node-run
   test scripts (e.g. all 32 capacity combinations × every tab, 252 assertions) — no
   device, renderer or emulator in the loop.
3. **Every bug found becomes a pinned regression.** The refresh-rotation race, the
   serde-bypasses-constructors hole, the default-privileges leak, the UTC+X measurement
   rejection — each survives as an assertion that fails if the lesson is unlearned.

## Alternatives considered

- **Conventional unit-test pyramid with mocked repositories.** Fast and familiar, but
  the claims that matter here live below the mock line; a mocked test of RLS or SQL
  grants asserts nothing. Kept for domain logic (163 `cargo test` cases) — rejected as
  the *primary* evidence.
- **End-to-end UI automation (Detox/Appium).** High maintenance, slow, flaky, and it
  tests the pixel layer while the risk sits in the data layer. The screenshot harness
  covers the visual record instead, without gating on it.
- **CI-only verification.** The workflow file exists, but a remote-less project cannot
  lean on it day-to-day; the local gate is the contract. CI re-runs the same scripts
  when a remote exists, adding no second source of truth.

## Consequences

- **Positive:** Refactoring is cheap and fearless; the documentation can say "every
  claim in this report is executable" and mean it; reviewers/markers can reproduce any
  claim with one command. The scripts double as living documentation of intended
  behaviour, including the failure modes.
- **Negative / costs:** Suites need live Postgres and a running server, so they are
  slower than unit tests and unsuitable for sub-second feedback; port and seed-state
  discipline is required (scripts create their own throwaway tenants); the suite count
  itself needs curating as features grow or `all-check` becomes a ritual rather than a
  gate.
- **Follow-ups:** Wire the same suites into hosted CI when a remote is configured;
  fold future offline-sync replay tests into the same pattern (replay a captured op-log
  against a live server and assert convergence).

## References

- [docs/project-log.md](../archive/project-log.md) — the record of what each suite caught
- [docs/assignment-report.md](../archive/assignment-report.md) §Testing — the suite table
- [ADR-0006](0006-immutable-program-versioning.md), [ADR-0004](0004-postgres-shared-schema-multitenancy.md) —
  the double-enforcement decisions these scripts exist to prove
