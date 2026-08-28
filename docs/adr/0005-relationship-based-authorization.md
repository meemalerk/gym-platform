# ADR-0005: Relationship-based authorization (OpenFGA)

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

Roles alone cannot express this domain. A single person may simultaneously be a member at
Gym A, an instructor at Gym B, a stand-in covering a class at Branch C, and the coach of a
specific set of members — with view access to programmes but not billing. Access depends on
*relationships* (who coaches whom, who owns what, which branch contains whom), not just a
global role.

## Decision

- Use **relationship-based access control (ReBAC) via OpenFGA** for the authorization graph.
- Combine it with **application-level domain checks**.
- Enforce a hard boundary:
  - **Authorization** answers *"may this actor attempt this action?"* → OpenFGA + app checks.
  - **Domain policy** answers *"is this action valid?"* → deterministic Rust.
- **Workout safety rules and domain validation never live in the authorization engine.**

## Alternatives considered

- **Global RBAC only** — cannot express per-relationship, multi-tenant, multi-role reality.
- **Cerbos (attribute/policy-driven)** — better when rules are primarily attribute
  conditions ("instructor cert current AND same branch AND draft AND below risk level"). Our
  hard part is the relationship graph, so OpenFGA fits better; attribute conditions we handle
  in domain policy (and OpenFGA conditions). Cerbos remains a documented alternative if
  attribute-heavy policy proliferates.

## Consequences

- **Positive:** naturally models multi-tenant, multi-role, relationship-driven access;
  supports RBAC and attribute-influenced rules through relationships and conditions;
  Zanzibar-style patterns are well documented.
- **Negative / costs:** an additional system to run and keep in sync; the authorization model
  is itself an artifact that must be versioned and migrated carefully.
- **Reinforced by** Postgres RLS as defence-in-depth
  ([ADR-0004](0004-postgres-shared-schema-multitenancy.md)).

## References

- [authorization-model.md](../authorization-model.md) (conceptual OpenFGA model)
