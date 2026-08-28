# Research Log — July 2026

> Findings from a parallel research sweep on 2026-07-14 to validate the stack against the
> current state of the art. **Caveat:** gathered via web-research agents; version numbers and
> dates should be re-verified at implementation time (`cargo info`, npm, official release
> notes). Where a finding changes a locked decision, it is promoted to an ADR. Items the
> researchers themselves could not confirm are marked *(unconfirmed)*.

Sections are filled as each research stream lands.

- [x] Backend & Rust
- [x] AI / LLM
- [x] Frontend / Web UI
- [x] Mobile (iOS / Android / Expo)
- [x] Security & compliance
- [x] Payments & app-store rules

---

## 1. Backend & Rust

### Verdict vs our plan
The locked stack (**Rust + Axum + SQLx + Postgres**, Postgres-backed jobs, OpenFGA, utoipa)
**holds up well** in mid-2026. No decision needs reversing. Three findings are worth
adopting or noting: SQLx's org move (pin carefully), a Rust-native AI-agent ecosystem now
exists, and durable-execution + CRDT options directly strengthen our AI-orchestration and
offline-sync designs.

### Language
- **Rust 1.97.0** (~2026-07-09) is latest stable; **edition 2024** (stable since 1.85) is
  current — brings **async closures** (`async || {}`, `AsyncFn*` traits) and better
  `impl Trait` lifetime capture.
- **`async fn` in traits**: static dispatch stable; **`dyn` async traits still need the
  `async-trait` crate** — use it for object-safe async abstractions (AI-provider, notifier,
  auth-backend ports).
- **`gen` blocks/generators remain unstable** — don't design around them.
- Edition 2027 is early-planning only.
- **Action:** target edition 2024 on Rust ≥1.85; keep `async-trait` for `dyn` ports.

### Web layer
- **Axum 0.8.x** still the Tokio-team default and the right call (tightest Tokio/Tower/Hyper
  alignment, biggest ecosystem). Actix, Salvo 0.89, Rocket 0.5.1 all production-fine; choice
  is team familiarity.
- **Pavex** (compile-time glue, Palmieri) is promising but still ~0.2 beta — revisit at 1.0.
- **Salvo** worth a look only if you want built-in HTTP/3 + auto-TLS + OpenAPI in one.
- **Tokio** ~1.52 (LTS 1.47/1.51 tracks); **Tower** 0.5.3; **Hyper** 1.x *(exact patch
  unconfirmed)*.
- **Action:** stay on Axum + Tokio + Tower + Hyper.

### SQL layer
- **SQLx 0.9.0** (2026-05) is latest — **note: the project moved orgs to `transact-rs/sqlx`**.
  0.9 adds a `SqlSafeStr` anti-injection trait, bumps MSRV to 1.86, and has breaking changes
  (can break SeaORM). **Pin carefully and read the changelog before upgrading.**
- Alternatives: **Diesel 2.3 + diesel-async 0.9** (stronger compile-time schema guarantees,
  more DSL friction); **SeaORM 2.0** (runtime-checked, built on SQLx); **cornucopia/Clorinde**
  (SQL files → typed Rust, alternative to inline `query!` macros); **Toasty** (Tokio's async
  ORM, v0.6, younger).
- **Action:** keep SQLx 0.9.x; the compile-time vs runtime-query discipline in
  [tech-stack.md](tech-stack.md) stands. Consider cornucopia if the team prefers SQL files
  over inline macros.

### Authorization
- **OpenFGA v1.17+** confirmed as a solid pick (REST-first, best multi-tenant docs, Auth0 FGA
  lineage). Two 2026 CVEs fixed — track releases. **SpiceDB v1.53** is the Zanzibar-purist
  strong-consistency alternative; **Cerbos 0.53** for attribute/policy-as-code; **Oso** has
  repositioned toward "AI agent security."
- **Important:** none are Rust-native — all accessed over gRPC/REST; **expect to write a thin
  internal Rust wrapper crate**. And the ReBAC engine does **not** stop your own code
  mis-querying — **pair with Postgres RLS** (this matches [ADR-0004](adr/0004-postgres-shared-schema-multitenancy.md)/[ADR-0005](adr/0005-relationship-based-authorization.md)).
- **Action:** OpenFGA + RLS confirmed. Add a wrapper-crate task.

### Background jobs
- **apalis** (with **apalis-pgmq** / **apalis-postgres**, `LISTEN/NOTIFY` low-latency
  dequeue, heartbeat recovery) on top of **pgmq 1.11** (Postgres-native queue) — **keeps jobs
  in the same Postgres transaction boundary as domain data, no Redis/RabbitMQ needed.**
- **Action:** this confirms and concretizes the "Postgres queue first" decision in
  [architecture.md](architecture.md) — name apalis + pgmq as the tools.

### OpenAPI
- **utoipa 5.5** is the actively-maintained default (first-class Axum). **aide**'s 2026
  maintenance cadence *(unconfirmed)*.
- **Action:** prefer utoipa over aide (update [tech-stack.md](tech-stack.md), which listed
  them as equal).

### Trends worth folding in
- **Durable execution — Restate** (itself built in Rust, single binary, embedded journal,
  Rust SDK) as a lightweight Temporal alternative. **Directly relevant to AI orchestration**
  (multi-step LLM tool-calling needing retry/resume) and offline-sync reconciliation. Temporal
  remains the heavyweight option. → candidate for a future ADR when Phase 5 (AI) starts.
- **Rust-native AI agent frameworks now exist:** **Rig** (rig.rs, unified API over 20+ LLM
  providers, OTel support) is the most prominent; **Swiftide** (typed task graphs, RAG);
  **AutoAgents** (actor model). Means the AI orchestration layer *can* stay in-process in Rust
  rather than calling a separate Python service. → informs [ai-authority-model.md](ai-authority-model.md).
- **CRDTs for offline sync — Automerge** (mature, Git-like history, Rust core) vs **Loro**
  (smaller encoded docs, younger). Relevant to [ADR-0008](adr/0008-offline-sync-operation-log.md):
  our operation-log design is still fine for append-heavy workout logging, but a CRDT lib is
  worth prototyping for document-like edits (programme drafts, coach notes). → note in ADR-0008
  as an option to evaluate.
- **WASM server-side** is real for edge/plugin niches but *not* a general backend replacement
  in 2026 — no action.

### Flagged unconfirmed
Exact hyper patch version; aide maintenance status; Permify as a ReBAC contender; edition
2027 scope. Re-verify versions at build time.

---

## 2. AI / LLM

### Verdict vs our plan
The AI-authority design ([ai-authority-model.md](ai-authority-model.md)) is **validated by the
research and, if anything, ahead of the curve**: the industry consensus in 2026 is exactly
"LLM emits a schema-shaped *proposal*; deterministic backend code is the safety gate; keep a
full audit trail." One meaningful correction to our earlier plan: **we can keep the whole
orchestration layer in Rust** — a viable Rust LLM ecosystem now exists, so we don't need a
separate Python AI service.

### Models (July 2026)
- Frontier tiers: **Claude Opus 4.8** (`claude-opus-4-8`, 1M ctx, $5/$25) / **Sonnet 5**
  (`claude-sonnet-5`, cheaper, near-Opus) / **Haiku 4.5** (routing/classification);
  **OpenAI GPT-5.1 and GPT-5.5** (5.6 *rumored/unconfirmed*); **Google Gemini 3.1 Pro / 3.5
  Flash / 3.1 Flash-Lite**.
- On tool-calling **policy adherence** (staying within constraints) a third-party Tau-Bench
  blog put Opus 4.8 ahead of GPT-5.5 and Gemini 3.5 Pro *(single-source, directional)* —
  relevant to our "bounded adjustments" requirement.
- **Recommendation:** Claude Opus 4.8 primary (Sonnet 5 for high-volume paths, Haiku 4.5 for
  intent routing); OpenAI GPT-5.5 as the documented fallback. This matches our platform model
  anyway.

### Structured outputs
- All three providers now do schema-constrained output; OpenAI/Gemini use true token-level
  grammar masking (hard guarantee), Anthropic's `strict: true` is very reliable and added
  constrained decoding late 2025.
- **Key point (already our stance):** schema-validity ≠ semantic-validity. "Increase squat
  400%" is schema-valid. **The Rust deterministic validator remains the real gate** — provider
  choice on this axis is therefore low-stakes. Confirms [ai-authority-model.md](ai-authority-model.md).

### Orchestration in Rust (change from earlier assumption)
- No heavyweight framework needed. The right pattern: an **explicit state machine**
  (propose → validate → confirm/reject → audit) in Rust, calling the provider's HTTP API.
- **Rust crates:** `genai` (multi-provider, native protocols — best starting point),
  `async-openai` (mature, OpenAI-compatible), raw `reqwest` for provider-beta features.
  **`rmcp`** is the official Rust MCP SDK (mature) if we later expose our validated tool
  surface via MCP.
- **MCP** is now mainstream (EMA enterprise-auth extension stable, adopted by Anthropic/
  Microsoft/Okta). Adopt **only if** multiple LLM clients need the same narrow tool surface —
  not required for a single embedded orchestrator. → informs a future AI ADR.
- **Durable execution:** pairs with **Restate** (from the backend research) for multi-step
  tool-calling that needs retry/resume.

### Auditability, safety, compliance
- **Self-hosted Langfuse** (OSS tracer) feeding our own audit log — keeps health/training
  traces on our infra (GDPR-friendly). Aligns with the audit tables in [domain-model.md](domain-model.md).
- **Governance:** NIST AI RMF + **ISO/IEC 42001** (becoming a procurement gate for enterprise/
  healthcare buyers in 2026).
- **Regulatory line to hold:** keep product language strictly **general wellness/fitness**
  (training load, volume, progression). Any diagnostic/treatment claim risks FDA device
  regulation (Jan-2026 FDA guidance). HIPAA generally does **not** apply to wellness apps but
  careful data handling still does. Our "narrow tools + deterministic validation + full audit"
  design is exactly the liability mitigation counsel would want. → feed into problem.md
  non-goals + security doc.

### Cost patterns
- **Prompt-cache** the stable system prompt + validation/tool schemas (up to ~90% off input);
  keep per-user dynamic data after the cache breakpoint.
- **Batch API** (~50% off) for non-interactive bulk work (nightly/weekly cohort re-planning).
  Interactive "review this adjustment now" stays synchronous+cached.

### On-device (mobile) AI
- iOS **Foundation Models framework** (on-device, now multimodal; can proxy to Claude/Gemini)
  and Android **Gemini Nano / AICore** are usable only for **low-stakes latency/privacy
  micro-tasks** (summarize a note, classify intent) — never the validated adjustment path.
  Android AICore is **foreground-only** (`BACKGROUND_USE_BLOCKED`). Device coverage is
  fragmented — design for graceful absence.

### Flagged unconfirmed
GPT-5.6 existence; exact open-model benchmark numbers; the Tau-Bench figures (single blog);
Anthropic strict-mode guarantee strength vs OpenAI — test empirically on real schemas.

---

## 2a. Self-hosted / no-per-token-fee AI (supersedes the model choice above → [ADR-0011](adr/0011-self-hosted-open-llm.md))

**Decision:** we self-host a small open-weight model — no paid frontier API. Justified because
the model is an untrusted proposer and Rust does the real validation, so the bar is
grammar-constrained JSON + tool-calling, not frontier reasoning. The frontier-model picks in §2
are demoted to an optional swap-in.

### The enabling finding
**All four self-hosted serving stacks (llama.cpp, Ollama, vLLM, SGLang) now GUARANTEE
JSON-schema-conformant output via grammar-constrained decoding** (GBNF / XGrammar / Outlines) —
mature and production-proven in 2026, not experimental. This is precisely the property our
design needs.

### Model (small, permissive-licensed)
- **Primary: Qwen3.5-9B** (Apache 2.0) — best-documented tool-calling lineage at its size.
- Alternatives (all Apache/MIT): **gpt-oss-20b** (OpenAI open, native tool-use, MXFP4 ~16GB),
  **Granite 4.1-8B** (IBM, explicit tool-calling + compliance/signed weights), **Mistral
  Small 4** (24B/~6B-active).
- Tight VRAM / CPU tier: **Qwen3.5-4B**, **Phi-4-mini**, or the function-calling-tuned
  **xLAM-2-3b**. MoE headroom: **Qwen3.5-35B-A3B** (~3B active).
- Licenses to note: Gemma (custom, use-restrictions), Llama 4 (Meta custom, >700M-MAU clause).
  DeepSeek has no strong small sibling — not a fit for cheap self-hosting.
- *(Model names/versions aggregator-sourced — verify on Hugging Face before committing.)*

### Serving + constrained decoding
- **Ollama** for local dev (schema-constrained `format` param, ~zero setup); **vLLM** or
  **SGLang** for prod. **SGLang + XGrammar** measured fastest for many small structured-JSON
  completions — our workload. Pass the Rust JSON Schema as `guided_json`/`format`; don't
  hand-roll grammars. (TGI is archived/maintenance-mode — avoid.)

### Rust glue
- Call the OpenAI-compatible endpoint via **`async-openai`** or **`genai`** (keeps a hosted
  fallback behind one abstraction). In-process Rust inference (**mistral.rs**, **candle**,
  **llama-cpp-2**) is possible but deferred — unnecessary complexity for v1.

### RAG / embeddings (when needed)
- Self-host **BGE-M3** (MIT) or nomic/Qwen-embed via Ollama; vectors in **pgvector on our
  existing Postgres** — no paid vector DB, no new infra. Gym-app RAG volume is small; runs on
  the same box (or CPU).

### Hardware & cost (honest — self-hosting ≠ free)
- **Owned:** a single **24GB RTX 4090 (~$1.5–2k one-time)** runs the whole 7–14B / MoE-3B-active
  tier **plus** embeddings. **Rented:** ~**$250–500/mo** 24/7 (RunPod), cheaper spot (Vast) or
  scaled-down off-hours — but renting *is* paying. **CPU-only** works for the 1–4B tier at
  multi-second latency (dev/low volume). No marginal per-token cost; not "free".

### Free hosted fallbacks — DEV ONLY
- Google AI Studio (Gemini Flash), Groq, OpenRouter free models, Cloudflare Workers AI — fine
  for prototyping, **but they log/train on prompts → never send real client health data**.
  Not a production answer.

### Recommended default
**Qwen3.5-9B** (or gpt-oss-20b) · **Ollama** (dev) → **SGLang/vLLM + XGrammar** (prod) ·
schema passed through as `guided_json` · **async-openai/genai** from Rust · **BGE-M3 +
pgvector** for RAG · one **24GB GPU** (owned or rented). Prefer deterministic domain logic over
the LLM wherever possible — it also shrinks the self-hosting bill.

---

## 4. Mobile (iOS / Android / Expo)

### Verdict vs our plan
RN + Expo holds. Two upgrades to our thinking: (1) **a custom Expo dev client is mandatory**
(not Expo Go) because of HealthKit/Health Connect/push — plan for it from day one; (2) for
offline sync, **PowerSync or WatermelonDB** are stronger, less error-prone choices than
hand-rolling the entire operation-log/sync protocol on raw expo-sqlite — see the ADR-0008
note below.

### Platforms (July 2026)
- **iOS:** iOS **26.6** shipping; **iOS 27** in public beta (GA ~Sept 2026). **Liquid Glass**
  design language becomes **mandatory once Xcode 27 ships (~Sept 2026)** — budget UI work.
  **HealthKit now runs live workout sessions on iPhone** (`HKWorkoutSession`/
  `HKLiveWorkoutBuilder`, previously Watch-only) + new **WorkoutKit**; iOS 27 adds workout
  **heart-rate/power zones**. watchOS "Workout Buddy" AI coaching exists. Implement **App
  Intents** ("log my workout" via Siri).
- **Android:** **Android 17** stable (June 2026). **Material 3 Expressive** rolling out —
  adopt but expect churn. **Health Connect** is the standard health hub. **Gemini Nano/AICore
  is foreground-only.** Check Android 17 behavior-changes (foreground-service, predictive back)
  before finalizing background sync.
- **Store age-verification APIs** (Apple Declared Age Range, Play Age Signals) rolling out per
  US state laws — wire in only if any teen-athlete use case.

### React Native / Expo
- **Expo SDK 57 / RN 0.86** appears latest *(SDK 57 weakly cross-verified — confirm at
  expo.dev/changelog)*; SDK 55+ is **New Architecture only** (legacy bridge removed) — **build
  on New Arch from day one, no opt-out exists.** Expo Router v7. EAS Build/Update mature;
  Hermes bytecode diffing shrinks OTA payloads (~75%) — good for shipping workout-logic fixes
  without store review.
- **Custom dev client required** (`expo-dev-client` + `expo prebuild`) — Expo Go can't load
  health/push native modules. **Action:** update [tech-stack.md](tech-stack.md) & mobile plan.

### Health / wearables
- No mature unified RN health package. Use **`@kingstinct/react-native-healthkit`** (iOS) +
  **`react-native-health-connect`** (Android) behind **our own thin abstraction** that unifies
  workout/steps/HR read-write.
- Wearables: Apple Watch via HealthKit+WorkoutKit; Garmin (official dev program, OAuth2/PKCE);
  WHOOP API; or aggregators (**Spike API / Open Wearables**) for multi-vendor. → future phase.

### Offline storage / sync (important for ADR-0008)
- Options: **PowerSync** (managed: streams Postgres→client SQLite with configurable sync rules
  + conflict handling — least custom code, fits our Postgres backend); **WatermelonDB**
  (batteries-included sync protocol, opinionated); **expo-sqlite + Drizzle** (right *foundation*
  either way, build sync yourself); **Legend-State**, **op-sqlite**, **Turso libSQL offline
  sync**.
- **Recommendation:** evaluate **PowerSync** first (managed conflict resolution, Postgres-
  native) or **WatermelonDB**, rather than hand-rolling the full op-log sync engine. Our
  operation-log *design* in [ADR-0008](adr/0008-offline-sync-operation-log.md) still governs the
  domain semantics (append-only sets, domain-specific conflicts) — but a proven engine can
  provide the transport. → add as an evaluation note/superseding consideration in ADR-0008.

### Push
- **`expo-notifications`** via Expo Push Service (→ FCM v1 / APNs). Constraints: ~600 msg/s
  per project, 4KB payloads, delivery receipts aren't proof of device delivery — do in-app
  receipt tracking for critical reminders. Requires custom dev build (SDK 53+).

### Flagged unconfirmed
Expo SDK 57/RN 0.86 exact status; Android 17 foreground-service & predictive-back specifics;
store commission rates (mid-litigation — see payments section); Health Connect API surface.
Re-verify before building.

---

## 3. Frontend / Web UI

### Verdict vs our plan
The web stack holds and sharpens. Confirmed: **React 19.2 + React Compiler (now GA — turn it
on), Vite (Rolldown-based), TanStack Router/Query, Zustand, Tailwind v4, dnd-kit, TipTap,
charts.** Two refinements: **skip Next.js/RSC entirely** (validated for our authenticated SPA),
and for the dense editable grid, seriously consider **AG Grid** over hand-building on TanStack
Table.

### Core
- **React 19.2** is the baseline (no React 20). **React Compiler is GA (v1.0, Oct 2025) —
  enable it**; a memo-heavy spreadsheet/calendar UI benefits most. **Skip RSC/Next App
  Router** — wrong tool for an interactive authenticated workspace.
- **Build:** **Vite 8** (Rolldown/Rust bundler, default) — sensible default, first-party React
  Compiler support, no framework lock-in. Rsbuild is a fine alt but no compelling reason over
  Vite. *(Vite 8 exact GA/perf figures marketing-sourced — verify.)*
- **Routing/state:** **TanStack Router** standalone (simplest) or **TanStack Start v1**
  (if we want server functions/SSR) — both beat Next App Router here. **TanStack Query v5**
  for server state + **Zustand v5** for UI/editing/drag state; add **Jotai** only if
  fine-grained cell-atom state gets unwieldy. Roll our own undo/redo command stack over Zustand.

### Styling / design system
- **Tailwind v4** (current; no v5) as the utility layer. **Avoid runtime CSS-in-JS** (decline +
  re-render cost) — matters for data-dense UI.
- **Headless primitives: Base UI 1.x** is the 2026 momentum leader (MUI-backed, Radix API
  lineage) and is now shadcn/ui's default; **Radix** still fine (now under WorkOS, slower).
  Use **React Aria Components (Adobe)** for the hardest a11y-critical widgets (grid cells,
  combobox-in-table) — its virtualized Table work is unusually relevant to us.
- **Recommendation:** own the design system, built on **shadcn tooling (CLI/registry) + Base UI
  + Tailwind v4**, reaching for React Aria on the hard widgets. Fits a dense, non-trendy admin
  aesthetic without a heavyweight component-lib dependency.

### Data-dense building blocks (the programme builder)
- **Editable grid:** **AG Grid** (Community is MIT/free; Enterprise ~$1k–2k/dev/yr for
  server-side row model, pivoting, Excel export) — fastest to production with built-in
  editors/keyboard-nav/range-select. **TanStack Table v8 + `@tanstack/react-virtual`** is the
  free, fully-headless alternative (you build editing UX). **Glide Data Grid** (canvas) only if
  row counts hit tens of thousands. *(Note: agents disagreed on a "TanStack Table v9" — v8 is
  the current stable production line; treat v9 as beta/not-shipped.)*
- **Drag & drop:** **dnd-kit** for exercise↔calendar-slot dragging (accessibility). Caveat: the
  widely-used legacy `@dnd-kit/core` v6.3.1 line is effectively frozen; the framework-agnostic
  rewrite (`@dnd-kit/react` v0.x) is pre-1.0. **Atlassian Pragmatic drag-and-drop** (v2, <4KB)
  is the stronger, scale-proven pick if maintenance risk matters.
- **Rich text:** **TipTap v3** (core MIT/free; Cloud tier only for collab/comments) — best DX
  for notes/instructions. Lexical only for extreme custom-editing scale.
- **Charts:** **Recharts v3** for standard progress/load/volume dashboards; **ECharts v6** only
  for very large time-series/specialized views; visx for bespoke chart types.

### Web platform features to use freely
Same-document **View Transitions** (all major browsers now), **container queries**, **Popover
API**, **`:has()`**. Use **CSS anchor positioning** only with a Floating-UI fallback
(Firefox/Safari lag); **scroll-driven animations** progressive-enhancement only.

### Flagged unconfirmed
Exact patch versions (Zustand/Jotai/RTK/TanStack Form); TanStack Table v9 status; Vite 8 perf
claims; anchor-positioning/scroll-animation cross-browser status. Re-verify at build time.

---

## 5. Security & compliance

### Verdict vs our plan
Our security posture is **well-aligned and, on the AI side, exactly what 2026 guidance
prescribes** ("narrow validated tools + per-call authz + keep secrets out of model context").
Key additions to fold into a dedicated security doc: an RLS transaction-scope pitfall to
enforce, the real compliance regime (not HIPAA but FTC HBNR + Washington MHMDA + GDPR Art. 9),
and the EU AI Act Article 50 transparency deadline (2026-08-02) that applies to our assistant.

### AuthN
- **Passkeys/WebAuthn** are mainstream — offer as first-class with a fallback.
- **Don't claim "OAuth 2.1"** — it's still an IETF draft. Build to **RFC 9700 BCP** (PKCE
  everywhere, exact redirect-URI match, refresh-token rotation, no implicit/password grants).
- **Web:** Backend-for-Frontend (BFF) — tokens server-side, browser gets an HttpOnly session
  cookie; avoid localStorage. **Mobile:** native public client + PKCE; tokens in Keychain/
  Keystore only.
- **Identity provider (B2B2C, gyms=tenants, many low-value end users):** **WorkOS/AuthKit**
  (free to ~1M MAU, charges only for enterprise SSO/SCIM) fits the cost curve best; **Clerk**
  (strong Organizations model) is the convenient default but pricier at scale; self-host
  **Zitadel** (org-first hierarchy — but **relicensed to AGPL-3.0 in v3**, evaluate for
  commercial use) or **Keycloak**. → informs a future identity ADR.

### Multi-tenant isolation (critical implementation detail)
- **RLS must use transaction-scoped context**: `set_config('app.current_tenant', $1, true)` /
  `SET LOCAL` inside the request transaction — **session-level `SET` leaks across pooled
  connections** (the #1 self-inflicted cross-tenant leak). The app DB role **must not own the
  tables and must not be superuser** (RLS is bypassed for owners/superuser).
- **Layer RLS (coarse tenant net) + OpenFGA ReBAC (fine per-resource)** — matches
  [ADR-0004](adr/0004-postgres-shared-schema-multitenancy.md)/[ADR-0005](adr/0005-relationship-based-authorization.md).
  Isolation most often silently breaks in **caches, background jobs, and search indexes**, not
  primary tables — enforce tenant context there too.

### AI security (reinforces [ai-authority-model.md](ai-authority-model.md))
- **OWASP Top 10 for LLM Apps 2025 (v2.0)** is current (no ratified 2026 edition); an **Agentic
  Apps Top 10** is emerging — track it.
- Prompt-injection defense is **architectural**, not a filter: separate readers (untrusted
  content) from doers (side-effect tools), scope tool grants per task, human-gate high-risk
  actions. **Treat all tool/RAG output as untrusted.**
- **Authorize the full (user, action, resource, tenant, task) tuple on every tool call — never
  trust agent-asserted tenant context.** "Scope translation" (resolving references only within
  the caller's tenant) is the step most skip.
- **Keep secrets out of the model context entirely** — then injection has nothing to exfiltrate.
- Guardrail options: NeMo Guardrails, Llama Guard 3, Guardrails AI — as a first-pass, not the
  boundary.

### Supply chain (matches the "cautious about supply-chain compromise" mandate)
- **Rust:** cargo-audit + cargo-deny on every PR; cargo-vet for the most sensitive crates.
- **npm:** publish with `--provenance` (Sigstore); lockfile-integrity + malware scanning
  (Socket) in CI.
- **SBOM:** generate CycloneDX 1.7 per release (EU Cyber Resilience Act now legally requires
  SBOMs for products sold into the EU).
- **Pin GitHub Actions to commit SHAs, not tags** (tj-actions/changed-files, CVE-2025-30066).
- Real 2026 attacks hit npm **and crates.io** (Shai-Hulud worm, axios compromise, TrapDoor
  across npm/PyPI/crates) — this is not hypothetical.

### Health-data compliance (real regime for us)
- **HIPAA generally does NOT apply** to a B2B2C wellness app (only via a BAA / acting for a
  covered entity). But these **do**: **FTC Health Breach Notification Rule** (covers non-HIPAA
  health/fitness apps, 60-day notice), **Washington My Health My Data Act** (biometric/health
  data, **private right of action**), and **GDPR Article 9** (readiness/injury/measurement =
  special-category → explicit, unbundled consent + DPIA).
- **EU AI Act:** **Article 50 transparency (disclose users are talking to AI) applies from
  2026-08-02** — near-term, applies to our assistant. High-risk Annex III obligations slipped
  to **2027-12-02** (Digital Omnibus). Avoid emotion/mental-state inference to stay out of
  high-risk.
- **App-store health rules:** HealthKit/Health Connect data — disclosed privacy policy, **no ad
  targeting, no sale to third parties**.
- **Data residency:** prefer **EU hosting + SCCs** for EU health data given the escalating legal
  challenge to the EU-US Data Privacy Framework.
- **Keep product language strictly "general wellness"** (reinforces the AI/FDA note in §2).

### Mobile security
- **expo-secure-store** (Keychain/Keystore) for small secrets only — **never bulk health data**
  (keep server-side / app-sandboxed encrypted DB). Wipe secrets on logout/delete (Keychain
  survives uninstall).
- Certificate pinning (react-native-ssl-public-key-pinning) + jailbreak/root detection
  (jail-monkey) as **defense-in-depth signals, never hard blocks** or a substitute for
  server-side validation.
- Target **OWASP MASVS 2.1** / MASTG-L2 profile (resiliency), not banking-grade MASTG-R.

### Secrets & infra
- **Infisical or Doppler** for app/CI secrets + **cloud KMS** for infra keys; avoid Vault unless
  enterprise scale. **SOPS + age** for GitOps-committed secrets. Automate rotation; scope every
  credential to least privilege + shortest lifetime.

### Flagged unconfirmed
Exact RFC 9700 number (verify); Trump v. Slaughter impact on FTC/DPF; EU AI Act classification
for our exact use case; MASTG profile mapping. Cited but verify before relying.

---

## 6. Payments & app-store rules

### Verdict vs our plan — this reshapes the mobile billing architecture
**Headline finding (high-confidence, cited to both stores' own policies):** a gym-membership /
1:1-coaching app **qualifies for external / web (Stripe) checkout and is exempt from app-store
IAP** on *both* platforms — **independent of the ongoing Epic litigation**:
- **Apple Guideline 3.1.3(d) "Person-to-Person Services" explicitly names "fitness training"**;
  3.1.3(e) covers services consumed outside the app.
- **Google Play Payments Policy lists "gym memberships" by name** as an exempt physical service,
  and exempts 1:1 "health coaching (personal trainer sessions)" when not recorded/replayable.

**Caveat:** any **purely digital, app-only content** (on-demand workout-video library, app-only
premium features) likely **still requires IAP** (Apple 3.1.1). A hybrid app must **segment SKUs**:
physical membership/coaching → Stripe; digital-only content → IAP. Confirm with App Store
Connect / Play Console and by checking how ClassPass/Mindbody/Peloton ship it.

**Impact:** the [subscriptions-billing.md](subscriptions-billing.md) entitlement model was right
to decouple access from payment source. We can bill gyms→members and members→app primarily via
**Stripe web checkout**, keeping IAP (via RevenueCat) only for any digital-only SKU. This avoids
the store cut on the core revenue.

### Stripe (2026)
- **Platform→Gym SaaS:** Stripe Billing (per-seat/per-branch/tiered/metered). **Meters API**
  replaced legacy Usage Records (needed for per-check-in/class metering). **Entitlements/Features
  API** for plan-gating *(verify GA vs preview status)*. No native parent-child customer
  hierarchy — model gym-chains with metadata-linked Customers (or see Chargebee Account
  Hierarchy).
- **Gym→Member:** **Stripe Connect** — **Accounts v2** (Dec 2025) unifies account types into
  role composition (a gym is both a Customer paying us and a Merchant collecting from members,
  one KYC). Direct/destination/separate charges + `application_fee_amount` for our platform fee.
  Embedded onboarding + networked KYC for multi-location gyms.
- **Tax:** Stripe Tax for the SaaS side; **the marketplace-facilitator tax rules on the
  Connect/gym-charges-members side are NOT fully handled by Stripe Tax** — pair with a dedicated
  engine (Anrok) or make a deliberate choice. Real gap, not a solved default.

### Mobile subscription infra
- **RevenueCat** still relevant (now also has **Web Billing**), but for us it's needed **only for
  any digital-only IAP SKU** — not the core membership flow (Stripe web). Alternatives: Adapty,
  Superwall, Qonversion. RevenueCat's own A/B test: native IAP converts better than web (~28% vs
  ~18%) — where IAP is legal-optional, still test conversion.
- **IAP↔entitlement reconciliation** (only if we sell digital SKUs): Apple App Store Server
  Notifications V2 (JWS-signed) + App Store Server API; Google Play RTDN via Pub/Sub. **Play
  Billing Library v8+ mandatory for updated apps by 2026-08-31.** Normalize both stores' state
  models into our one internal entitlement object — matches our entitlement design.

### Billing modeling & gym specifics (confirms subscriptions-billing.md)
- **Entitlements pattern is industry-standard:** decouple "what was purchased" from "what's
  allowed," persist entitlements locally, sync via webhook (don't call the provider live per
  request). 3-layer model: catalog/entitlements + metering + billing, with a low-latency local
  cache.
- **Freezes:** Stripe has both "pause payment collection" (stays active) and a newer **Pause
  Subscription** endpoint (suspends invoicing + access, 2026-05). **Class packs:** credit-based
  one-time products decremented on redemption (distinct from recurring dues). **Family plans:**
  one payer + discounted add-on line items (hardest edge case: mid-cycle proration).
- **Dunning:** Stripe Smart Retries + Card Account Updater baseline (~25% of lapses are
  involuntary).

### Compliance
- **PCI DSS v4.0.1** — using Stripe Elements/Checkout keeps SAQ A, but new **Req 6.4.3 / 11.6.1**
  (payment-page script inventory + tamper detection) now apply. **We still never touch card
  data.**
- **SCA/3DS (PSD2):** Stripe auto-applies 3DS2; recurring membership charges after an initial
  authenticated payment qualify as merchant-initiated (exempt from repeat SCA). **PSD3/PSR** in
  the EU pipeline may revise thresholds — watch.
- **Cancellation:** FTC "click-to-cancel" rule was vacated (July 2025) but the FTC enforces the
  same substance via Section 5/ROSCA and states have parallel laws — **build "cancel as easily as
  you signed up" regardless.**

### Flagged unconfirmed
Stripe Entitlements API GA status; Apple's eventual US external-link commission rate (SCOTUS cert
granted, argued ~Oct 2026 term — could change within the year); hybrid-app SKU segmentation
(verify in store consoles); marketplace-facilitator tax handling; FTC click-to-cancel & PSD3
both moving. Re-verify before launch.

---

## Cross-cutting synthesis — what changed vs the original plan

1. **Nothing fundamental was reversed.** Rust+Axum+SQLx+Postgres, modular monolith, shared-schema
   tenancy, OpenFGA+RLS, immutable versioning, bounded-AI, offline-first, RN+Expo/React web — all
   validated by 2026 research.
2. **Biggest new insight (billing):** gym membership + 1:1 coaching are **IAP-exempt** on both
   stores → bill via Stripe/web, avoid the store cut; IAP only for any digital-only SKU. → new
   payments ADR.
3. **AI orchestration can stay in Rust** (genai/async-openai/rmcp + Restate for durable steps) —
   no separate Python service needed. Our "narrow validated tools + audit" design is exactly
   2026 best practice. **AI is self-hosted (open-weight, no per-token fee) — see §2a / ADR-0011.**
4. **Offline sync:** evaluate a proven engine (**PowerSync** / WatermelonDB) for transport rather
   than hand-rolling; our op-log domain semantics still govern. → note in ADR-0008.
5. **Concrete tool picks now named:** apalis+pgmq (jobs), utoipa (OpenAPI), Vite 8 + Base UI +
   Tailwind v4 + AG Grid/TanStack Table + dnd-kit/Pragmatic + TipTap + Recharts (web),
   custom Expo dev client + kingstinct HealthKit / react-native-health-connect (mobile),
   WorkOS/Clerk (identity), Infisical/Doppler (secrets), Langfuse (AI observability).
6. **Compliance regime clarified:** not HIPAA by default, but FTC HBNR + WA MHMDA + GDPR Art. 9;
   EU AI Act **Article 50 transparency applies 2026-08-02**; keep language "general wellness."
7. **One critical implementation guardrail surfaced:** RLS must be **transaction-scoped**
   (`SET LOCAL`, non-owner role) or tenancy leaks across the connection pool.

> All version numbers/dates here are 2026-07-14 research snapshots from web-research agents —
> **re-verify against primary sources at implementation time.**
