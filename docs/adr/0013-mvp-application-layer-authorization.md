# ADR-0013: Application-layer authorization for the MVP (OpenFGA deferred)

- **Status:** Accepted
- **Date:** 2026-07-18
- **Deciders:** Project author
- **Relates to:** [ADR-0005](0005-relationship-based-authorization.md) (not superseded — deferred)

## Context

[ADR-0005](0005-relationship-based-authorization.md) locks **OpenFGA** as the authorization
engine, because the domain is genuinely relationship-driven (a person can be a member at Gym
A, an instructor at Gym B, and cover a class at Branch C).

That reasoning still holds. But at MVP scope the relationship graph is exactly one edge —
`user --role--> gym` — expressed by the `memberships` table. Standing up OpenFGA now would
add a service to run, a model to version, and a network hop per request, to answer a question
a single indexed SQL lookup already answers correctly. Research also confirmed OpenFGA is not
Rust-native, so it needs a wrapper crate regardless
([research-2026.md](../research-2026.md) §1).

## Decision

For the MVP, **authorize in the application layer**, behind a boundary that keeps ADR-0005's
architecture intact:

- The **`TenantScope` extractor** resolves the caller's role **from the database on every
  request** (never from the token), producing a `TenantContext`.
- **Domain policy stays separate from authorization**, exactly as ADR-0005 requires: the
  extractor answers *"may this actor attempt this?"*; `Exercise::new` and the services answer
  *"is this action valid?"*.
- Role capability lives in **one place** (`StaffRole::can_manage_catalogue`), not scattered
  through handlers — so swapping the decision source later touches few call sites.
- **Postgres RLS remains the planned second layer** ([ADR-0004](0004-postgres-shared-schema-multitenancy.md));
  application-level `gym_id` filtering is implemented and tested today.

**Adopt OpenFGA when** any of these become true:
1. permissions depend on relationships beyond `user → gym` (coach ↔ athlete, branch
   hierarchies, programme-level sharing);
2. a second service needs to ask the same authorization questions;
3. per-resource sharing/delegation is required.

## Alternatives considered

- **Stand up OpenFGA now** — architecturally "correct" but premature: real operational cost for
  a one-edge graph, and it would slow MVP iteration. Deferring is reversible; the port boundary
  is what protects us.
- **Scatter role checks in handlers** — rejected. It works initially and then rots into
  inconsistent, unauditable policy.

## Consequences

- **Positive:** no extra service to run or version; one indexed query per request instead of a
  network call; MVP ships sooner; the auth/domain-policy split from ADR-0005 is preserved and
  already tested (cross-tenant reads return 404, role gate returns 403).
- **Negative / costs:** authorization logic lives in Rust rather than a declarative model, so
  richer relationships will require the migration this ADR defers. The longer we wait, the more
  call sites move.
- **Guardrail:** keep authorization decisions out of handlers and out of the domain
  constructors. If a role check starts appearing inline in a handler, that is the signal this
  ADR needs revisiting.
- **Follow-up:** implement RLS policies with a non-owner DB role and transaction-scoped
  `SET LOCAL` (the pooling pitfall in [research-2026.md](../research-2026.md) §5) — tracked in
  the roadmap.

## References

- [ADR-0004](0004-postgres-shared-schema-multitenancy.md), [ADR-0005](0005-relationship-based-authorization.md),
  [authorization-model.md](../authorization-model.md), [research-2026.md](../research-2026.md) §1/§5
