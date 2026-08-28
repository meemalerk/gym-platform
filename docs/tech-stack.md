# Tech Stack

The choices here are locked by ADRs in [adr/](adr/). This is the consolidated reference.
Reasoning lives in the ADRs; don't re-litigate here — write a superseding ADR if a choice
needs to change.

> **Version numbers below are July-2026 research snapshots** — see
> [research-2026.md](research-2026.md) for sources and caveats. **Re-verify against primary
> sources at implementation time** (`cargo info`, npm, official release notes). The stack
> *shape* is locked by ADRs; specific versions are not.
>
> ✅ **Verified against the registries on 2026-07-18** and now actually building. The
> "Pins that cost us a bug" section below is not tidy-up-able — each entry is load-bearing.

## Practices audit (2026-07-18)

Checked our code against current published guidance. **Confirmed still correct:**

- **`#[async_trait]` for `Arc<dyn Repository>` is NOT outdated.** Native `async fn` in traits
  (stable since 1.75) is still not `dyn`-compatible, and Return Type Notation — the mechanism
  expected to fix it — had its stabilization PR **closed unmerged in Dec 2025**. Keep
  `async_trait` for object-safe ports; use plain native `async fn` for static dispatch.
  (`dynosaur` is the alternative if per-call boxing ever shows up in a profile.)
- **`thiserror` 2.x + `anyhow`** remains the consensus split.
- **Axum extractors use native `async fn`, not `#[async_trait]`** — which is what
  `crates/api/src/extract.rs` already does (axum 0.8's own traits moved to RPITIT).
- **`{param}` path syntax** (axum 0.8) — already used.

**Corrected as a result of this audit:**

| Was | Now | Why |
|-----|-----|-----|
| Redirect-guard in `(app)/_layout.tsx` | **`<Stack.Protected guard={…}>`** in the root layout | The current Expo Router auth pattern. Flipping the guard drops history, so a signed-out user cannot "back" into the app. Client-side only — the server remains the authority. |
| No connectivity/focus wiring | **`onlineManager` + `focusManager`** wired to `expo-network` + `AppState` | On native, TanStack Query cannot detect reconnect or foreground by itself; without this, queries silently fail to refetch. |
| React Compiler off | **`experiments.reactCompiler: true`** | Now viable and worth enabling on a greenfield app — no legacy manual memoization to conflict with. Verified by a successful bundle export. |
| `sha2 = "0.10"` | **`sha2 = "0.11"`** | 0.11.0 *is* stable; my earlier "ecosystem split" reasoning was wrong (it duplicates, it does not break). |

**Deliberately not adopted:**

- **`use()` + Suspense for data fetching in RN** — React 19's docs push this, but RN support is
  still unreliable in 2026 (open upstream issues). TanStack Query is the right tool here;
  reserve Suspense for lazy component loading.
- **New Architecture is not a toggle any more.** RN 0.82 removed the old bridge and SDK 55+
  ignores `newArchEnabled`. Any doc phrasing it as a choice is stale.

**To watch:** SQLx 0.9 added `SqlSafeStr` — runtime (non-macro) queries now require
`AssertSqlSafe(...)` for dynamically-built SQL. That will matter when we add the dynamic
filtering/analytics queries described below; our current code is all compile-time macros.

## Pins that cost us a bug (do not "modernise" these)

| Dependency | Pinned | Why not the newest |
|------------|--------|--------------------|
| `argon2` | **0.5** | 0.6 is still a release candidate (`0.6.0-rc.8`). We do not ship auth on a pre-release. |
| `sha2` | **0.10** | 0.11 moves to the `digest` 0.11 ecosystem and splits from argon2 0.5. |
| `jsonwebtoken` | **10.4, `default-features = false`, `features = ["rust_crypto"]`** | v10 ships **no crypto provider by default** (`default = ["use_pem"]`) and **panics at first token issue**. It compiles fine — only a runtime test catches it. `rust_crypto` is pure Rust, so CI and distroless images need no C toolchain. `use_pem` dropped: we sign with an HMAC secret, not PEM keys. |
| jsonwebtoken `leeway` | **5s (explicit)** | The library defaults to **60s**, silently extending every access token's life. |
| `react-dom` | **19.2.3 (matches `react`)** | Expo pins `react@19.2.3`, but a transitive `react-dom@19.2.7` demands `react@^19.2.7`, breaking every `npm install`. Let `expo install` choose it. |
| TypeScript (mobile) | **~6.0.3 (Expo's choice)** | Latest is 7.0.2, but Expo SDK 57 aligns on 6.x. Mismatched versions are a classic RN breakage. `openapi-typescript` peer-requires TS ^5.x, so it runs via `npx` as a codegen step instead of being a project dependency. |

## Backend — Rust ([ADR-0002](adr/0002-backend-language-rust.md))

```
Rust 1.97+ (edition 2024)   async closures; use `async-trait` for dyn ports
Axum 0.8          HTTP framework (thin, Tower/Hyper based)
Tokio 1.5x        async runtime (LTS tracks 1.47/1.51 if stability wanted)
Tower / tower-http middleware: timeouts, tracing, compression, authz
Serde             (de)serialization
Garde             validation (or `validator`)
SQLx 0.9          compile-time-checked SQL, no ORM (note: moved to transact-rs org)
PostgreSQL        primary datastore
utoipa 5.5        OpenAPI generation (preferred over aide)
apalis + pgmq     Postgres-backed background jobs (no Redis — see architecture.md)
OpenFGA 1.17+     relationship-based authorization (wrap in a thin Rust crate; + RLS)
OpenTelemetry     tracing/metrics/logs
tracing           structured logging
```

Alternatives noted by research if the team prefers: **cornucopia/Clorinde** (SQL files →
typed Rust) instead of inline `sqlx::query!`; **Diesel + diesel-async** for stronger
compile-time schema guarantees. **Restate** (Rust durable-execution engine) is a candidate
for multi-step AI orchestration and sync reconciliation when Phase 5 starts.

**Chosen because:** the team already knows Rust; the domain has genuinely complex state
where "invalid states unrepresentable" pays off; the recommendation engine is product
infrastructure; strong tenant/coach/member boundaries matter; future computational
features (analysis, sync logic) stay in one ecosystem.

**Sanctioned fallback — Go** ([ADR-0002](adr/0002-backend-language-rust.md)) if team
composition shifts toward delivery speed over domain control. Go stack would be: Go 1.26,
Chi + Huma, pgx, sqlc, River (jobs), OpenFGA, OpenTelemetry, Postgres. **Not Fastify** —
sharing a language with the mobile client is not sufficient reason when the backend is the
core product.

### SQLx discipline

| Compile-time SQL (`sqlx::query!`) | Runtime query builder |
|-----------------------------------|-----------------------|
| Commands | Instructor filtering |
| Permission-sensitive reads | Analytics screens |
| Billing / subscription ops | Search |
| Workout & programme mutations | Configurable reports |

Don't try to prove the entire domain at compile time.

## Member mobile app ([ADR-0009](adr/0009-client-stack.md))

*Actually installed in `apps/mobile` (2026-07-18): expo 57.0.7 · react-native 0.86.0 ·
react 19.2.3 · expo-router 57.0.7 · @tanstack/react-query 5.101 · zustand 5.0 · zod 4.4 ·
react-hook-form 7.82 · @shopify/flash-list 2.0.2 · reanimated 4.5 · gesture-handler 2.32 ·
expo-secure-store 57.0.1.*

```
React Native 0.86  Expo SDK 57 (New Architecture only — no legacy opt-out)
                   ⚠ CUSTOM DEV CLIENT REQUIRED (expo-dev-client + prebuild) —
                   Expo Go can't load expo-secure-store / HealthKit / Health Connect
                   ⚠ SDK 56+: expo-router no longer supports importing from
                   @react-navigation/* — route all navigation through expo-router
Expo Router v57    TypeScript
EAS Build/Update   OTA JS fixes bypass store review (Hermes bytecode diffing)
TanStack Query     server state
Zustand            local state
React Hook Form + Zod
Reanimated · FlashList
Offline sync       evaluate PowerSync (managed, Postgres-native) or WatermelonDB
                   rather than hand-rolling the op-log transport; expo-sqlite +
                   Drizzle is the foundation either way (see ADR-0008 note)
Health             @kingstinct/react-native-healthkit (iOS) +
                   react-native-health-connect (Android), behind our own thin
                   abstraction; no mature unified package exists
Push               expo-notifications (Expo Push Service → FCM v1/APNs)
NativeWind         styling + custom design system
```

**Platform notes (July 2026):** iOS 26 ships / iOS 27 in beta; **Liquid Glass becomes
mandatory once Xcode 27 ships (~Sept 2026)** — budget UI work. HealthKit now runs live
workout sessions on iPhone (`HKWorkoutSession`) + WorkoutKit. Android 17 stable; Material 3
Expressive; Gemini Nano/AICore is foreground-only.

## Instructor & admin web ([ADR-0009](adr/0009-client-stack.md))

```
React 19.2 + React Compiler (GA — turn it on)
Vite 8            Rolldown-based bundler
TanStack Router   (or TanStack Start v1 if server functions/SSR wanted)
TanStack Query v5 · TanStack Table v8 (+ @tanstack/react-virtual)
Editable grid     AG Grid (Community MIT; Enterprise paid) OR TanStack Table+Virtual
Tailwind v4 + Base UI (headless; shadcn tooling) + React Aria for hard a11y widgets
dnd-kit           drag/drop (or Atlassian Pragmatic DnD for scale — see research)
TipTap v3         rich text (notes, docs)
Zustand (UI state) · Jotai (only if fine-grained cell-atoms) · Zod
Recharts v3       charts (ECharts v6 only for very large/specialized views)
```

Desktop-first. **Not Next.js / RSC** — no meaningful public SEO surface; the authenticated
workspace gains nothing from RSC. Use platform features freely: same-document View
Transitions, container queries, Popover API, `:has()`.

## Type sharing without a shared language

```
Rust API types  →  OpenAPI  →  generated TypeScript SDK  →  mobile + web
```

This replaces the "shared language" advantage a TS backend would have offered.

## Infrastructure

```
PostgreSQL      AWS RDS / Neon / Crunchy Bridge (EU hosting for EU health data)
Redis           only when genuinely required
Object storage  Cloudflare R2 or S3
Deployment      Fly.io or AWS ECS (Kubernetes later, only if needed)
CDN / WAF       Cloudflare
Identity        WorkOS/AuthKit (cost-fits B2B2C) or Clerk; self-host Zitadel/Keycloak
Secrets         Infisical or Doppler + cloud KMS (SOPS+age for GitOps); avoid Vault early
AI observability Langfuse (self-hosted — keeps health traces on our infra)
Monitoring      Grafana + OpenTelemetry + Sentry
Email           Resend or Postmark
Push            Expo Notifications initially
Payments        Stripe (Billing + Connect Accounts v2) for membership/coaching (IAP-exempt);
                RevenueCat + IAP only for digital-only SKUs; Stripe Tax + Anrok (marketplace)
Supply chain    cargo-audit + cargo-deny (+ cargo-vet); npm --provenance; CycloneDX SBOM;
                pin GitHub Actions to commit SHAs
```

See [ADR-0010](adr/0010-payments-and-billing.md) for the payments decision and
[research-2026.md](research-2026.md) §5 for the full security/compliance picture (RLS
transaction-scope pitfall, FTC HBNR + WA MHMDA + GDPR Art. 9, EU AI Act Art. 50 by
2026-08-02).

## AI — self-hosted, no per-token fees ([ADR-0007](adr/0007-ai-authority-levels.md), [ADR-0011](adr/0011-self-hosted-open-llm.md))

We **self-host a small open-weight model** — no paid frontier API, no per-token bill. This is
sound *because* the model is an untrusted proposer and the Rust validator is the real safety
gate (see [ai-authority-model.md](ai-authority-model.md)); a small model that does reliable
grammar-constrained JSON + adequate tool-calling is enough.

```
Model            Qwen3.5-9B (Apache 2.0) primary — mature tool-calling; alts: gpt-oss-20b,
                 Granite 4.1-8B, Mistral Small 4. Tight VRAM/CPU: Qwen3.5-4B / xLAM-2-3b.
                 MoE headroom: Qwen3.5-35B-A3B (~3B active).
Serving          Ollama (local dev) → vLLM or SGLang (prod). SGLang + XGrammar is fastest
                 for high volume of small structured-JSON completions (our workload).
Constrained decoding  pass the Rust JSON Schema as guided_json (vLLM/SGLang) / format
                 (Ollama) — GUARANTEED schema-conformant output. Don't hand-roll grammars.
Orchestration    IN-PROCESS in Rust — explicit state machine (propose → validate →
                 confirm/reject → audit); no separate Python/AI service.
Rust glue        async-openai or genai against the OpenAI-compatible endpoint; rmcp if we
                 later expose our validated tool surface via MCP.
RAG (when needed) self-host BGE-M3 (or nomic/Qwen-embed) via Ollama; vectors in pgvector
                 on our existing Postgres — no paid vector DB, no new infra.
Tool calling     narrow, application-defined domain tools only
Deterministic validation  every proposal validated by domain policy (the real gate)
Instructor-configurable authority policies (levels 0–3)
Full decision audit trail (+ self-hosted Langfuse tracing)
```

**Hardware reality (not free, but no per-token fee):** one 24GB RTX 4090 (~$1.5–2k owned, or
~$250–500/mo rented) runs the whole 7–14B tier + embeddings; the 1–4B tier can run CPU-only
for dev/low volume. **Free hosted tiers (Groq/Gemini/OpenRouter) are dev-only — never send
real client health data through them** (they log/train on prompts).

The model never receives raw DB mutation tools (`execute_sql`, `update_workout`,
`delete_program`). Schema-validity ≠ semantic-validity — the Rust validator is the boundary.
Keep secrets out of the model context. Product language stays **"general wellness"** (FDA
line). Prefer deterministic domain logic over the LLM wherever it fits — it also keeps the
self-hosting bill small. See [ai-authority-model.md](ai-authority-model.md) and
[research-2026.md](research-2026.md) §2.

## API style

REST + OpenAPI externally; WebSockets or SSE for live changes; internal domain events. No
gRPC to mobile (adds proxying/debugging/compat cost without helping normal workout CRUD).
Internal gRPC is possible later for computational services — Tower/Tonic share the
middleware model with Axum.

## Deliberately deferred

Kafka · database-per-tenant · Redis-by-default · ClickHouse · NATS JetStream ·
wearables/live telemetry. Each is pulled forward only against concrete need — see
[roadmap.md](roadmap.md) Phase 6+.
