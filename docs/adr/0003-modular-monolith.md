# ADR-0003: Modular monolith over microservices

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

The platform has many capabilities (identity, coaching, programming, execution, scheduling,
billing, AI, notifications, audit). It is tempting to split these into services early, but
we are pre-scale, want strong transactional guarantees (e.g. transactional outbox), and need
fast iteration on a tightly interrelated domain.

## Decision

We will build a **modular monolith**: one deployable, internally organised by **business
capability**, with a strict inward dependency direction.

```
crates: domain ← application ← api / worker;  infrastructure implements application ports
modules by capability: identity, organisations, branches, memberships, coaching,
exercises, programmes, scheduling, workouts, readiness, progress, classes,
subscriptions, messaging, notifications, ai, audit
```

Dependencies point inward: `api → application → domain`; `infrastructure` implements ports
declared by `application`. The domain knows nothing of HTTP/SQL/AI.

## Alternatives considered

- **Microservices from day one** — premature; distributed transactions, operational
  overhead, and network boundaries before we have the scale or team to justify them.
- **Unstructured monolith** — fast initially but rots into a big ball of mud without the
  capability boundaries and dependency rules.

## Consequences

- **Positive:** transactional integrity (outbox in the same DB), fast local development,
  clear seams that *could* later be extracted if a capability genuinely needs independent
  scaling.
- **Negative / costs:** requires discipline to keep module boundaries clean and not let
  everything import everything.
- **Rule:** do not turn every database table into a "service." Organise around business
  capabilities. Extract a service only against concrete scaling/deployment need.

## References

- [architecture.md](../architecture.md)
- Move to independently deployed consumers / NATS only when warranted — see
  [ADR-0004](0004-postgres-shared-schema-multitenancy.md) and [roadmap.md](../roadmap.md).
