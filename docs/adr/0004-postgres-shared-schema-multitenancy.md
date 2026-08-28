# ADR-0004: Single PostgreSQL, shared schema, `gym_id` tenancy

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

This is a multi-tenant platform where gym organisations are tenants. We need strong tenant
isolation as a safety property, but we do not yet have enterprise customers with contractual
data-isolation requirements. Database-per-tenant creates operational misery before there are
enough customers to justify it.

## Decision

- **One PostgreSQL database, one shared schema.**
- **`gym_id` on every tenant-owned table**, indexed.
- **Application-level authorization is the primary enforcement** of tenant isolation.
- **Row-level security (RLS) keyed on `gym_id` as defence-in-depth** — the backstop, not the
  primary mechanism.
- **Every repository operation requires tenant context** in its signature:
  `find_program(tenant: GymId, program: ProgramId)`, never `find_program(program_id)`.

## Alternatives considered

- **Database-per-tenant** — strongest isolation, but heavy operational cost (migrations,
  connection management, provisioning) with no payoff at our scale. Reserved for very large
  enterprise / contractual isolation later.
- **Schema-per-tenant** — middle ground, still multiplies migration/operational surface;
  not justified now.

## Consequences

- **Positive:** simple operations, one migration path, easy cross-tenant platform queries
  for the owner; the mandatory tenant-context signature eliminates a whole class of
  cross-tenant bugs at the type level; RLS means a leak requires two failures.
- **Negative / costs:** noisy-neighbour and blast-radius considerations are shared; large
  tenants can't be isolated without migrating to per-DB later.
- **Follow-ups:** revisit database-per-tenant only for enterprise isolation contracts
  ([roadmap.md](../roadmap.md) Phase 6+).

## References

- [domain-model.md](../domain-model.md) (example table with `gym_id` + index),
  [architecture.md](../architecture.md), [authorization-model.md](../authorization-model.md)
