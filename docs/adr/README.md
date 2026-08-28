# Architecture Decision Records

An ADR captures a single significant decision: the context, the choice, and the
consequences. They are the durable memory of *why* the system is the way it is — so that a
new session (human or Claude) does not re-litigate settled choices or accidentally
contradict them.

## Process

1. **One decision per record.** Copy [`0000-template.md`](0000-template.md) to
   `NNNN-short-slug.md` with the next number.
2. **Status** starts as `Proposed`, becomes `Accepted` when agreed, `Superseded by
   ADR-XXXX` when replaced. Never edit an Accepted ADR's decision — supersede it with a new
   one.
3. **Link it** from this index below and, if foundational, from
   [../../CLAUDE.md](../../CLAUDE.md).
4. Keep it short. Context → Decision → Consequences. Trade-offs stated honestly.

## To reverse or change a decision

Do **not** silently start doing the opposite. Write a new ADR that references and supersedes
the old one, mark the old one `Superseded by ADR-XXXX`, and update CLAUDE.md.

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-backend-language-rust.md) | Backend language: Rust + Axum (Go as fallback, not Fastify) | Accepted |
| [0003](0003-modular-monolith.md) | Modular monolith over microservices | Accepted |
| [0004](0004-postgres-shared-schema-multitenancy.md) | Single Postgres, shared schema, `gym_id` tenancy | Accepted |
| [0005](0005-relationship-based-authorization.md) | Relationship-based authorization (OpenFGA) | Accepted |
| [0006](0006-immutable-program-versioning.md) | Immutable programme versioning | Accepted |
| [0007](0007-ai-authority-levels.md) | Tiered, bounded, validated AI authority | Accepted |
| [0008](0008-offline-sync-operation-log.md) | Offline-first sync via operation log | Accepted |
| [0009](0009-client-stack.md) | Client stack: RN+Expo mobile, React+TanStack web | Accepted |
| [0010](0010-payments-and-billing.md) | Payments & billing (Stripe; gym membership is IAP-exempt) | Accepted |
| [0011](0011-self-hosted-open-llm.md) | Open-weight LLM; serverless-first, self-host at scale | Accepted |
| [0012](0012-domain-data-rag.md) | Domain knowledge via RAG over our own DB (not fine-tuning) | Accepted |
| [0013](0013-mvp-application-layer-authorization.md) | App-layer authz for MVP (OpenFGA deferred, not dropped) | Accepted |
| [0014](0014-identity-capacities-and-profiles.md) | One account, many capacities; profiles are person-owned | Accepted |
| [0015](0015-gym-operating-calendar.md) | One operating calendar: weekly pattern + dated overrides | Accepted |
| [0016](0016-nutrition-scope.md) | Nutrition is coach-authored guidance, never generated prescription | Accepted |
| [0017](0017-deterministic-recommendations.md) | Recommendations are deterministic rules with readable reasons | Accepted |
| [0018](0018-computed-progress-and-goals.md) | Progress is computed from immutable history, never stored | Accepted |
| [0019](0019-verification-first-development.md) | Verification-first development against live infrastructure | Accepted |
| [0020](0020-design-system.md) | Design system: verified contrast, scheme-specific containment, one focal element | Superseded in part by [0030](0030-signal-design-language.md) |
| [0021](0021-browser-demo-distribution.md) | Demos ship as a same-origin browser build behind a tunnel | Accepted |
| [0022](0022-seed-demonstrates-the-rules.md) | The seed demonstrates the invariants, not just the happy path | Accepted |
| [0023](0023-single-gym-deployment.md) | Single-gym deployment: keep the tenancy engine, cap it to one gym | Accepted |
| [0024](0024-trainer-authority.md) | Trainer authority is three rights, not one: authoring vs publishing vs curating | Amended by [0034](0034-who-prescribes-and-who-consents.md) — authoring moves to managers |
| [0025](0025-coaching-requests.md) | A coaching relationship takes consent from both sides | Superseded by [0031](0031-standing-not-invitations.md) |
| [0026](0026-open-registration.md) | Both doors: open registration alongside invitations | Amended by [0031](0031-standing-not-invitations.md) — one door |
| [0027](0027-outbox-and-worker.md) | A worker, a transactional outbox, and a hand-rolled Postgres queue | Accepted |
| [0028](0028-self-hosted-card-gateway.md) | A card gateway the deployment hosts itself, behind ADR-0010's seam | Accepted |
| [0029](0029-auth-hardening.md) | Password reset, email verification, and a login throttle | Accepted |
| [0030](0030-signal-design-language.md) | The Signal design language: violet-indigo brand, one reserved lime, containment by fill and elevation | Accepted |
| [0031](0031-standing-not-invitations.md) | One door in, standing set from the roster; coaching pairs without a handshake | Accepted |
| [0032](0032-staff-accounts.md) | An owner can create a staff account outright, with a generated one-time password | Accepted |
| [0033](0033-group-classes-and-self-service.md) | Group classes as weekly slots with derived sittings; a member may start a programme, and leave a plan, alone | Accepted |
| [0034](0034-who-prescribes-and-who-consents.md) | The trainer prescribes for their own clients, the gym curates the catalogue, and a gym-proposed pairing takes the trainer's consent | Accepted |
| [0035](0035-unplanned-sessions.md) | A session's plan link is optional: a member with no coach builds their own workout, and everything downstream is untouched | Accepted |
| [0036](0036-three-capacities.md) | Three capacities — owner, trainer, member; `admin` and `head_coach` removed | Accepted |
