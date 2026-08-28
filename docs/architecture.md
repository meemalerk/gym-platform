# Architecture

See [tech-stack.md](tech-stack.md) for the concrete technology choices and
[adr/](adr/) for the reasoning. This document is the structural map.

## System overview

```
   React Native member app        React instructor / admin web
   (Expo, offline-first)          (Vite + TanStack, desktop-first)
              │                              │
              └──────────────┬───────────────┘
                             │  REST + OpenAPI (JSON)
                             │  WebSockets / SSE for live updates
                             ▼
                   ┌─────────────────────┐
                   │  Rust modular        │
                   │  monolith (Axum)     │
                   │                      │
                   │  api → application   │
                   │        → domain      │
                   │  infrastructure      │
                   │  implements ports    │
                   └──────────┬───────────┘
                              │
              ┌───────────────┼────────────────┐
              ▼               ▼                 ▼
        PostgreSQL       OpenFGA          Worker (same
        (single DB,      (ReBAC authz)    binary/crate,
        shared schema)                     drains outbox)
```

The TypeScript SDK for both clients is **generated from the backend's OpenAPI spec** —
this is how we get type sharing without sharing a language (see
[ADR-0009](adr/0009-client-stack.md)).

## Backend: modular monolith

One deployable, internally organised by **business capability**, not by table and not as
microservices ([ADR-0003](adr/0003-modular-monolith.md)).

### Crate layout (Cargo workspace)

```
crates/
  domain/          # pure domain types, invariants, no I/O. Enums that make
                   # invalid states unrepresentable live here.
  application/     # use-cases / command handlers; depends on domain; defines
                   # ports (traits) it needs from infrastructure.
  infrastructure/  # adapters: SQLx repositories, Argon2 hashing, JWT issuing.
                   # Later: outbox, object storage, AI provider.
  api/             # Axum HTTP layer + extractors + OpenAPI. Thin.
bins/
  server/          # entrypoint: config, wiring, graceful shutdown
  # worker/        # NOT YET — arrives when there are outbox events to drain
```

**As built (2026-07-18).** `worker` is deliberately absent: adding an empty binary before
there are events to process is speculative structure.

### Request-time authorization

`TenantScope` (an Axum extractor) resolves the caller's role **from the database on every
request** rather than reading it from the token. That costs one indexed query and buys
immediate effect for revoked memberships. Non-members receive **404, not 403** — a 403 would
confirm the gym exists to an outsider. See [ADR-0013](adr/0013-mvp-application-layer-authorization.md)
for why OpenFGA is deferred rather than dropped.

### Refresh-token rotation is compare-and-swap

`SessionRepository::revoke` returns whether *this* call performed the revocation, implemented
as a single conditional `UPDATE ... WHERE revoked_at IS NULL`. Losing that race is treated as
theft and burns the token family.

This is not incidental: a read-then-write version let **two concurrent refreshes both
succeed** with the same token. Sequential tests could not see it; `scripts/verify-refresh-hazard.mjs`
reproduces it. The mobile client correspondingly de-duplicates concurrent refreshes into a
single in-flight request.

### Module map (business capabilities)

```
modules/
  identity/        organisations/   branches/       memberships/
  coaching/        exercises/       programmes/     scheduling/
  workouts/        readiness/       progress/       classes/
  subscriptions/   messaging/       notifications/  ai/         audit/
```

Modules are logical groupings within the crates, organised by capability. Do **not** turn
every table into a "service."

### Dependency direction (strict)

```
   api
    ↓
   application
    ↓
   domain
```

`infrastructure` implements the ports that `application` declares. Dependencies point
inward; the domain knows nothing about HTTP, SQL, or the AI provider.

## The HTTP layer is thin

Axum + Tower/tower-http keep the HTTP layer minimal; the real application is ordinary Rust
modules. Middleware (timeouts, tracing, compression, authorization) is Tower layers. The
Tower model means the same middleware concepts port to Tonic/gRPC services if internal
gRPC is ever introduced.

## Data access

- **Every repository operation requires tenant context**: `find_program(tenant: GymId,
  program: ProgramId)`, never `find_program(program_id)`. This eliminates a whole class of
  cross-tenant bugs ([ADR-0004](adr/0004-postgres-shared-schema-multitenancy.md)).
- **Compile-time SQL** (`sqlx::query!`) for commands, permission-sensitive reads, billing,
  workout mutations.
- **Runtime query builders** for dynamic filtering, analytics, search, configurable
  reports — where literal-SQL macros can't cleanly apply.

## Authorization boundary

Two distinct questions, two distinct layers ([authorization-model.md](authorization-model.md)):

- **"May this actor attempt this action?"** → OpenFGA (relationship-based) +
  application-level checks.
- **"Is this action valid?"** → domain policy (deterministic Rust). Workout safety rules
  live here, **never** in the authorization engine.

## Events & background work

Transactional outbox pattern:

```
BEGIN
  <state change>            -- e.g. update workout
  INSERT INTO outbox (...)  -- domain event, same transaction
COMMIT
        │
        ▼
   Worker drains outbox → notifications, AI analysis, weekly summaries,
   analytics projections, calendar generation, coach alerts
```

Start with a Postgres-backed queue. Move to NATS JetStream only when there are multiple
independently deployed consumers or high-volume live telemetry. **No Kafka.**

Representative events: `ProgramPublished`, `ProgramAssigned`, `WorkoutScheduled`,
`WorkoutStarted`, `WorkoutCompleted`, `PainReported`, `ReadinessDeclined`,
`PersonalRecordAchieved`, `CoachApprovalRequested`, `MemberSubscriptionExpired`.

## AI orchestration lives inside the backend

Not a separate microservice (initially). The flow:

```
User message → intent classification → permission check → load training context
  → call approved domain tools → validate proposed action → persist decision + evidence
  → return explanation
```

The model is given **narrow business operations**, never raw DB mutation tools like
`execute_sql` / `update_workout` / `delete_program`. Details in
[ai-authority-model.md](ai-authority-model.md).

## Three product surfaces

1. **Member mobile app** — React Native + Expo, offline-first (Expo SQLite + operation
   log). Today / in-session logging / progress / assistant.
2. **Instructor web app** — React + Vite + TanStack, desktop-first. The programme builder
   is a spreadsheet + calendar + structured-document hybrid, not a stack of modal forms.
3. **Gym administration surface** — role-sensitive navigation; shares the web app shell.

## What we deliberately avoid early

Kafka, database-per-tenant, Redis-unless-needed, gRPC-to-mobile, Next.js. See
[problem.md](problem.md) non-goals and [tech-stack.md](tech-stack.md).
