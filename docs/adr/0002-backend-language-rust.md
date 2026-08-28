# ADR-0002: Backend language — Rust + Axum (Go as fallback, not Fastify)

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

The backend is the core product, not CRUD around an LLM. The domain has genuinely complex
state: structured/versioned programmes, progression and scheduling logic, recommendation
validation, permission-safe data access, offline-sync logic, high-volume analytics, and
possible future movement analysis. The team already knows Rust and Go. TypeScript's main
draw (sharing a language with the mobile client) is weak here because the backend's value is
domain correctness, not API plumbing — and types can be shared via generated SDKs anyway.

A Fastify backend was an earlier, too-generic suggestion. It suits a small consumer app, a
TS-heavy team, a shallow permission model, and a product whose value is the chatbot. None of
those describe this product.

## Decision

We will build the backend in **Rust with Axum + Tokio + Tower + SQLx on PostgreSQL**.

**Go is the sanctioned fallback** if team composition shifts so that organisational delivery
speed matters more than domain-level control. The Go stack would be: Go 1.26, Chi + Huma,
pgx, sqlc, River, OpenFGA, OpenTelemetry, Postgres.

**Fastify is rejected** as the default backend.

## Alternatives considered

- **Go** — excellent delivery speed, tooling, concurrency, operational simplicity; weaker
  expressive modelling of complex domain states (interfaces / nullable-field structs vs.
  Rust enums). Strong second choice, kept as the documented fallback.
- **Fastify (TypeScript)** — good AI SDK convenience and fast prototyping, but "shares a
  language with mobile" is insufficient reason when the backend is the core product. Third
  choice.

## Consequences

- **Positive:** the type system makes invalid domain states unrepresentable; SQLx gives
  compile-time-checked SQL without an ORM; strong performance and memory predictability;
  future computational features stay in one ecosystem.
- **Negative / costs:** slower development for dynamic reporting, rapid AI-provider
  experimentation, SDK integrations, admin CRUD, and schema plumbing. Compile times and
  dense async type errors. Hiring depends on market. SQLx's static macros can't cleanly
  express fully dynamic queries.
- **Discipline required:** use `sqlx::query!` (compile-time) for commands, permission-
  sensitive reads, billing, and workout/programme mutations; use runtime query builders for
  instructor filtering, analytics, search, and configurable reports. Do not try to prove the
  whole domain at compile time.

## References

- [tech-stack.md](../tech-stack.md), [architecture.md](../architecture.md)
- Axum (Tower/Hyper middleware model); SQLx compile-time query checking.
