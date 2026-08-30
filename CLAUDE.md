# CLAUDE.md — Gym Platform

> Read this first. It is the anchor for every session so we do not drift. If a decision
> here conflicts with something you're about to do, stop and reconcile it (update the doc
> or challenge the decision) before writing code.

## What this is

A **single-gym coaching and gym-management platform** — not a workout tracker. Built on
multi-tenant-*capable* infrastructure (`gym_id`, RLS, `TenantScope` — see ADR-0004), but
each deployment is capped to exactly one gym (ADR-0023). Within that one gym:

```
The gym
  ├── Administrators
  ├── Head coaches
  ├── Instructors
  └── Members
```

The competitive advantage is the **programme model, instructor workflow, adjustment
engine, and longitudinal training data** — not the chatbot. Keep workout logic
deterministic and testable. The AI is the most talkative doorway into those systems, not
the system itself.

## The plan lives in `docs/`

| Doc | What it holds |
|-----|----------------|
| [docs/product-specification.md](docs/product-specification.md) | **What the product is** — users, capabilities, the invariants, and what it refuses to do |
| [docs/developer-guide.md](docs/developer-guide.md) | **How to work on it** — layout, the five things that bite, how to write a feature end to end |
| [docs/delivery-stages.md](docs/delivery-stages.md) | The stages, what each proves, and what "done" means |
| [docs/problem.md](docs/problem.md) | Problem statement, users, product vision, non-goals |
| [docs/roadmap.md](docs/roadmap.md) | Phased delivery plan and current phase |
| [docs/architecture.md](docs/architecture.md) | System architecture, module map, dependency direction |
| [docs/domain-model.md](docs/domain-model.md) | Entities, program lifecycle, storage model |
| [docs/tech-stack.md](docs/tech-stack.md) | The chosen stack and why |
| [docs/authorization-model.md](docs/authorization-model.md) | ReBAC model, auth vs. domain policy boundary |
| [docs/ai-authority-model.md](docs/ai-authority-model.md) | AI authority levels 0–3, guardrails, tool contract |
| [docs/subscriptions-billing.md](docs/subscriptions-billing.md) | B2B2C billing: SaaS + memberships + entitlements |
| [docs/cost-analysis.md](docs/cost-analysis.md) | Cost model, AI economics, unit economics, break-even, profitability |
| [docs/hosting-deployment.md](docs/hosting-deployment.md) | Hosting/infra options + costs + deploy approach |
| [docs/market-analysis.md](docs/market-analysis.md) | Market size, competitor pricing, positioning gap |
| [docs/feature-plan-2026-07.md](docs/feature-plan-2026-07.md) | **The current build order.** Maps every requested capability to a phase, with dependencies |
| [ORIGIN.md](ORIGIN.md) | The prototype→product pivot: where this codebase came from |
| [docs/archive/](docs/archive/) | Archived prototype-era docs (assignment report, project log) — reference only |
| [screenshots/README.md](screenshots/README.md) | Indexed captures of every view per role, from the live app |
| [research/INDEX.md](research/INDEX.md) | Downloaded primary sources, each annotated with the decision it grounds |
| [docs/glossary.md](docs/glossary.md) | Domain vocabulary — use these terms exactly |
| [docs/research-2026.md](docs/research-2026.md) | July-2026 stack research + sources (re-verify versions) |
| [docs/adr/](docs/adr/) | Architecture Decision Records — the *why* behind every hard choice |

**Decisions are recorded as ADRs.** Before proposing a change to a foundational choice
(language, tenancy, auth, versioning), read the relevant ADR. To reverse one, write a new
ADR that supersedes it — do not silently contradict it.

## Decisions already locked (see ADRs for the reasoning)

1. **Backend: Rust + Axum + SQLx + PostgreSQL.** Go is the sanctioned fallback if team
   composition shifts toward delivery speed over domain control. Not Fastify. → [ADR-0002](docs/adr/0002-backend-language-rust.md)
2. **Modular monolith**, not microservices. Organise by business capability, not by table. → [ADR-0003](docs/adr/0003-modular-monolith.md)
3. **Single Postgres DB, shared schema, `gym_id` on every tenant-owned table**, RLS as
   defence-in-depth. Every repository call takes tenant context. → [ADR-0004](docs/adr/0004-postgres-shared-schema-multitenancy.md)
4. **Relationship-based authorization (OpenFGA)** + application-level domain checks.
   Auth answers "may this actor attempt this?"; domain policy answers "is this valid?" → [ADR-0005](docs/adr/0005-relationship-based-authorization.md)
5. **Immutable programme versions.** Editing a published version creates a new draft;
   assignments reference a specific version. → [ADR-0006](docs/adr/0006-immutable-program-versioning.md)
6. **AI authority is explicit and tiered (0–3), instructor-configurable, always
   deterministically validated.** The model never gets raw DB mutation tools. → [ADR-0007](docs/adr/0007-ai-authority-levels.md)
   The model is a **small open-weight LLM (not frontier Claude/GPT)** with grammar-constrained
   JSON — sound *because* Rust is the real validator. **Cost-optimal deployment = cheap
   serverless open-model APIs** (DeepInfra/Together, ~½¢ per member/mo); self-host a GPU only at
   high scale or for data-control. Domain knowledge = **RAG over pgvector, not fine-tuning.** →
   [ADR-0011](docs/adr/0011-self-hosted-open-llm.md), [ADR-0012](docs/adr/0012-domain-data-rag.md)
7. **Offline-first member app via an operation log** with on-device UUIDv7/ULID IDs and
   domain-specific conflict resolution (never blanket last-write-wins). → [ADR-0008](docs/adr/0008-offline-sync-operation-log.md)
8. **Clients:** React Native + Expo (member app); React + Vite + TanStack (instructor/admin
   web). TypeScript SDK generated from the backend's OpenAPI spec. → [ADR-0009](docs/adr/0009-client-stack.md)
9. **Payments: Stripe** — gym memberships & 1:1 coaching are **app-store-IAP-exempt** (both
   stores name the category), so bill via Stripe web/Connect and avoid the store cut; IAP only
   for digital-only SKUs. → [ADR-0010](docs/adr/0010-payments-and-billing.md)
10. **One operating calendar**: a recurring weekly pattern plus dated overrides, resolved by one
    function. Opening hours are `TIME` in the gym's IANA timezone, never `timestamptz` (that
    breaks across DST, silently and seasonally). No RRULE. → [ADR-0015](docs/adr/0015-gym-operating-calendar.md)
11. **Nutrition is coach-authored guidance, never generated prescription.** No AI meal plans, no
    calorie targets, nothing keyed on a medical condition — it collides with the AI non-goal in
    problem.md and carries real regulatory and human risk for no differentiation.
    → [ADR-0016](docs/adr/0016-nutrition-scope.md)
12. **Recommendations are deterministic rules with readable reasons** — no learned ranking, no
    LLM in the suggestion path; every suggestion carries a `because`. → [ADR-0017](docs/adr/0017-deterministic-recommendations.md)
13. **Progress is computed from immutable history, never stored** — est-1RM/BMI/goal progress
    are derived on read; goals target observable metrics only, baseline captured at creation.
    → [ADR-0018](docs/adr/0018-computed-progress-and-goals.md)
14. **Verification-first development**: every feature ships an executable verification script
    against live server + DB; bugs become pinned regressions. → [ADR-0019](docs/adr/0019-verification-first-development.md)
15. **Design system — the "Signal" language**: a violet-indigo brand plus **one electric
    lime that only ever means *live*** (a running rest timer, a session in progress);
    semantic colour is a **triplet** (`danger`/`dangerHi`/`dangerInk`, same for success and
    warn) so a status chip is a token lookup rather than a per-screen invention;
    **containment is fill and elevation, never an outline** — `rule` is a control edge held
    to 3:1, `line` is a decorative hairline; corners come from `t.radius`; Bricolage
    Grotesque over Plus Jakarta Sans. The palette is **verified against WCAG AA by a
    script** (100 pairings, including opacity composites), the shape rules are **asserted by a second script**, and both
    were changed in the same commit as the decision.
    → [ADR-0030](docs/adr/0030-signal-design-language.md), superseding the palette and shape
    halves of [ADR-0020](docs/adr/0020-design-system.md) (whose *method* it keeps)
16. **Trainer authority is three rights, not one** — authoring (coach-level, because a
    draft binds nobody) vs publishing vs curating the catalogue. Exercises a trainer adds
    land as `proposed` and are usable immediately, because progress is computed per
    `exercise_id` and a duplicate permanently splits an athlete's history.
    → [ADR-0024](docs/adr/0024-trainer-authority.md)
17. **A member chooses their coach, and that is the whole of it** — no handshake. The
    pairing and its record land in one transaction; the coach can end it, and still cannot
    assign *themselves* a client (that would be a self-service grant of access to somebody
    else's data). A trainer *directory* is not the member roster.
    → [ADR-0031](docs/adr/0031-standing-not-invitations.md), superseding
    [ADR-0025](docs/adr/0025-coaching-requests.md)
18. **Two ways to make staff, for the two real cases.** For somebody who has no
    account, an owner **creates it outright** — account and standing in one
    transaction, with a generated one-time password handed over and never stored
    in readable form. For somebody who already has one, they walk in and get
    promoted (below); an address that already exists is a **409, not a merge**,
    because attaching a stranger's account to your gym is a membership without
    consent. Everybody can now change their own password, which is what makes a
    handed-over one safe to hand over.
    → [ADR-0032](docs/adr/0032-staff-accounts.md)
19. **One door in, and standing is set afterwards.** Invitations are **gone** — table,
    routes and all. Everybody walks through the open door (owner-controlled, default
    closed) as a `member`; a manager then sets what they hold from the roster. Only an
    owner may grant or remove `owner`, the last owner cannot step down, and standing is
    **replaced** rather than amended so promotion and demotion cannot disagree.
    → [ADR-0031](docs/adr/0031-standing-not-invitations.md),
    [ADR-0026](docs/adr/0026-open-registration.md)
20. **A worker, a transactional outbox, and a hand-rolled Postgres queue** —
    `FOR UPDATE SKIP LOCKED`, no apalis, no pgmq, no Redis. Recurring billing, overdue
    notices and coach alerts all hang off it. → [ADR-0027](docs/adr/0027-outbox-and-worker.md)
21. **A card gateway the deployment hosts itself**, behind ADR-0010's seam — a real
    implementation with the bank replaced, so the whole payment path is demonstrable and
    testable without an account anywhere. → [ADR-0028](docs/adr/0028-self-hosted-card-gateway.md)
22. **Password reset, email verification and a login throttle** — single-use hashed tokens
    in the invitation's shape, refusals that never confirm who exists, and failures counted
    per address and per origin in the database. → [ADR-0029](docs/adr/0029-auth-hardening.md)
23. **Single-gym deployment.** This is one gym, not a platform of many — `gyms` is capped
    to one row by a DB trigger, active only when `SINGLE_GYM_MODE=true` (set explicitly in
    `docker-compose.demo.yml`; off by default so `scripts/verify-rls.sh` can still
    create several gyms to prove tenant isolation). The tenancy *engine* (`gym_id`, RLS, `TenantScope`) is unchanged; only the
    choice to have more than one gym is gone. → [ADR-0023](docs/adr/0023-single-gym-deployment.md),
    supersedes part of [ADR-0014](docs/adr/0014-identity-capacities-and-profiles.md)

24. **Group classes are weekly slots; a booking is a place in one dated sitting.**
    Occurrences are derived in SQL, never minted ahead. Capacity is stated in the domain
    and made true by a partial unique index. Cancelling is a timestamp, so a place can be
    re-booked and a past roster stays readable. **And three authority checks that were
    correct for acting on other people stopped being applied to acting on yourself**: a
    member may put themselves on a published programme, take themselves off it, and end
    their own subscription. Review also now counts *prescribed exercises* rather than
    weeks — counting containers let empty programmes reach published, where they freeze.
    → [ADR-0033](docs/adr/0033-group-classes-and-self-service.md)

25. **The trainer prescribes; the gym curates; a proposed pairing needs consent.** Three
    authority checks that were correct for acting on *other people* were being reused for
    the wrong question, and the result was that the owner could do everything, so did.
    Assignment is now the athlete's own coach (by relationship) or the athlete themselves —
    **not a manager**. Authoring the catalogue moves to head coach and above (reversing
    ADR-0024's authoring half; a trainer reads it and assigns from it, so five trainers
    cannot fragment the data with five near-duplicate programmes). And a manager pairing a
    trainer with a member now **proposes** — direct creation is gone — with only the named
    trainer able to accept, because that pairing hands them somebody's whole training
    history. → [ADR-0034](docs/adr/0034-who-prescribes-and-who-consents.md)

26. **A session's plan link is optional — a member with no coach builds their own
    workout.** Someone on the **Open Gym** membership holds `gym_access` and nothing
    else: no coach, so nobody to prescribe, so (with `assignment_id NOT NULL`) no
    session, so the app recorded **nothing** for the membership gyms sell most of.
    `assignment_id`/`workout_template_id` are now nullable **both-or-neither** (CHECK
    constraint + two domain constructors + a 400), with a `title` only an unplanned
    session may carry. Nothing downstream changed: `performed_sets.template_exercise_id`
    was already nullable ("work outside the plan"), and est-1RM, history, goals and
    attendance all compute per `exercise_id` without reading the prescription.
    Called **unplanned**, not "open" — `SessionStatus::is_open()` already means "in
    progress" on the same type. → [ADR-0035](docs/adr/0035-unplanned-sessions.md)

27. **Three capacities, not five: owner, trainer, member.** `admin` was "owner minus
    the right to make other owners"; `head_coach` was "the catalogue", a rung ADR-0024
    invented and ADR-0034 then moved authoring *to*. Nobody occupied either — the demo
    seeded neither, and their one reliable home was the standing picker. `can_manage_gym`
    and `can_manage_catalogue` both resolve to `is_owner` now and are **kept as separate
    methods**: they ask different questions and would diverge again if a rung returned.
    Rows were mapped **asymmetrically on purpose** — `admin`→`owner` (escalation, because
    locking a gym's administrator out is worse than one extra right), `head_coach`→
    `trainer` (demotion, because promoting every senior coach to owner to save the
    catalogue is a far larger grant). Refused at three layers: API, CHECK constraint, and
    `Capacity::parse` returning `None` — which deliberately does *not* map the old strings,
    since `parse` is what turns a row into authority.
    → [ADR-0036](docs/adr/0036-three-capacities.md)

**Stack versions** were validated against July-2026 research — see
[docs/research-2026.md](docs/research-2026.md). Version numbers there are snapshots;
**re-verify against primary sources before relying on them.** The stack *shape* is locked by
ADRs; specific versions are not. Key research-driven refinements: AI is a **self-hosted
open-weight model** (no per-token fee — Qwen3.5-9B-class via Ollama/vLLM/SGLang with
grammar-constrained JSON), orchestrated in-process in Rust (no Python service); background jobs
= apalis + pgmq; identity =
WorkOS/Clerk; offline sync should evaluate PowerSync/WatermelonDB rather than hand-rolling
transport; **RLS must be transaction-scoped** (`SET LOCAL`, non-owner DB role) or tenancy
leaks across the pool; EU AI Act Art. 50 (disclose AI) applies from 2026-08-02.

## Working principles

- **Invalid states should be unrepresentable.** Lean on Rust's type system — enums over
  nullable-field bags for domain state (program status, exercise prescription, etc.).
- **Compile-time SQL (`sqlx::query!`) for** commands, permission-sensitive reads,
  billing, and workout mutations. **Runtime query builders for** dynamic filtering,
  analytics, search, configurable reports. Do not try to prove the whole domain at
  compile time.
- **Tenant context is non-optional** in the repository layer. `find_program(tenant,
  program_id)`, never `find_program(program_id)`.
- **Transactional outbox** for events: state change + event insert in one transaction; a
  worker drains it. Start with a Postgres-backed queue; NATS JetStream only when there
  are genuinely independent consumers.
- **REST + OpenAPI externally**, WebSockets/SSE for live updates. No gRPC to mobile.
- **Don't reach for heavy infra early.** No Kafka, no database-per-tenant, no Redis until
  a concrete need exists. This is a gym platform, not a bank.

## Current status

**This is a product, not an assignment** (withdrawn from assessment 2026-07-25 — see
[ORIGIN.md](ORIGIN.md); the pre-pivot history with full attribution lives on the
`archive/assignment` branch). **Phase 3 core complete** (assignment & execution; offline
transport still owed). Phases 0–1 complete; Phase 2's model/API complete (authoring UI
owed); Phase 2.5 complete. Current focus: **the product UI redesign — done 2026-08-24** (ADR-0030: both clients
rebuilt on the Signal language; the `gym-app/Gym App Views.dc.html` reference is now
history, not a target), then entitlements/billing. Check the "Current phase" marker in
[docs/roadmap.md](docs/roadmap.md) before starting implementation work, and see
[README.md](README.md) for how to run things.

Working today (all verified against a live database — `scripts/all-check.sh` runs **39
reporting suites**: 283 unit tests (248 domain), 57 e2e assertions over **79** OpenAPI
paths, and roughly 1,450 assertions in total across the per-feature verify scripts listed
below. Everything green as of 2026-08-31):

**Added 2026-08-24** — each with an ADR and an executable suite:

- **Trainer authority split three ways** (ADR-0024). A trainer writes programmes and names
  movements; a head coach publishes and curates. Exercises gain
  `proposed | approved | retired` and a review queue. `verify-trainer-authority.sh` (34).
- **Coaching requests and a trainer directory** (ADR-0025). The member asks, the coach
  answers, and accepting creates the pairing in the same transaction.
  `verify-coaching-requests.sh` (46).
- **Open registration** (ADR-0026), owner-controlled and default closed, alongside
  invitations. Grants `member` and nothing else. `verify-open-registration.sh` (28).
- **The athlete detail screen** and honest workout durations. `ended_at` now travels with
  the data, so a session synced hours late no longer reports the sync delay as training
  time. Session list gains athlete/date filters. `verify-athlete-view.sh` (25),
  `verify-attendance.mjs` (44).
- **The `worker` crate, the outbox and recurring billing** (ADR-0027). `next_charge_on`
  finally advances; arrears settle in one pass; overdue invoices are noticed exactly once
  without being mutated. `verify-worker.sh` (31).
- **A self-hosted card gateway** (ADR-0028) behind ADR-0010's seam, so the whole payment
  path is testable and demonstrable with no account anywhere. `verify-payments.sh` (36).
- **Password reset, email verification, login throttling** (ADR-0029), plus an
  `EmailSender` port whose only adapter records rather than sends — stated, not hidden.
  `verify-auth-hardening.sh` (36).
- **The gym operating calendar** (ADR-0015, designed long ago and now built): weekly
  pattern plus dated overrides, one resolution rule, trainer availability in the same
  shape. `verify-calendar.sh` (51).
- **`apps/console`** — the React/Vite owner-and-coach web app ADR-0009 specified. Its
  tokens are **generated** from `apps/mobile/src/ui/theme.ts` — colour, radius, elevation
  and both type families — and `all-check.sh` fails if the checked-in file drifts, so the
  two clients cannot disagree about shape any more than about colour. **The console is
  deliberately almost icon-free** (ADR-0030): it is read rather than browsed, and a glyph
  beside a word in a table is a second thing to decode. Status is a pill with the status
  written in it.
- **The verification harness now runs on Windows.** Eleven suites hard-coded `/tmp/b.json`
  (a different directory for bash and for a native Python), called `python3` (the Store
  stub, which prints nothing and exits 0), and read/printed non-UTF-8. Every one of them
  reported passes it had never run. Two `.mjs` scripts crashed on
  `new URL(...).pathname`.

- Cargo workspace `crates/{domain,application,infrastructure,api}` + `bins/{server,worker}`.
  The `worker` crate drains the ADR-0027 outbox (recurring billing, overdue notices).
- Postgres 17 via `docker-compose` on **host port 5455** (5432/5433 are taken by other
  projects on this machine). Migration `20260718000001_init`: gyms, users, memberships,
  exercises, sessions.
- **Auth**: sign-up provisions the account only (Argon2id passwords, short-lived HS256
  access tokens, opaque **rotating** refresh tokens stored only as SHA-256 hashes, reuse
  detection revoking the whole token family); gym + owner is a separate, one-time
  `POST /api/v1/gyms` call (see the single-gym bullet below — that call now only succeeds
  once, ever).
- **Identity (ADR-0014) + single gym (ADR-0023)**: **one account, capacities as a set**
  (`owner`/`admin`/`head_coach`/`trainer`/`member`) — so the gym can have several owners,
  and someone can be trainer *and* member at once. Profiles (`athlete_profiles`,
  `trainer_profiles`) are **person-owned, no `gym_id`**. What changed under ADR-0023: this
  is a **single-gym deployment** — `gyms` is capped to one row (DB trigger,
  `SECURITY DEFINER`, active only when `SINGLE_GYM_MODE=true` so verification paths that
  need several gyms are unaffected), so "a trainer across many gyms" and "solo users get a
  personal gym" no longer apply. `/api/v1/me` still returns `memberships` as a list (the
  backend stays multi-gym-observable on purpose — `verify-rls.sh` needs that); the
  **mobile client** is what commits to singular, taking the first entry.
  **All permission questions go through `Capabilities::can_*` — never inspect the raw set.**
- **Tenancy**: `TenantScope` extractor re-reads capacities from the DB per request
  (never trusts the token). Holding none gives **404, not 403**, so tenant existence isn't leaked.
- **RLS is live** (`migrations/…_row_level_security.sql`) — the DB enforces tenant isolation
  too. Verified by `scripts/verify-rls.sh` (7 checks). Two rules that make it real: the app
  must connect as a **non-owner role** (owners bypass RLS entirely), and tenant context must be
  **transaction-scoped** via `set_config(..., true)`. Policies use `nullif(...,'')` because a
  committed GUC resets to empty string, not NULL.
- **Standing** (ADR-0031, replacing invitations): everybody joins through the open door as a
  `member`, and `PUT /gyms/{id}/members/{user}/capacities` sets what they hold. The whole set
  is sent, not a delta, so "promote" and "demote" are one call. `check_standing_change` holds
  the rules — only an owner touches `owner`, the last owner cannot step down, a standing
  cannot be emptied — and revocation is a `revoked_at` stamp, never a delete, so the audit
  trail can still answer what somebody could do last Tuesday.
- **Exercises**: tenant-scoped catalogue, capability-gated writes.
- **Programme authoring UI (Phase 2's owed half, now built)**: `programs.tsx` → `new-program`
  → `add-week` → `add-workout` → `prescribe`, with the lifecycle as buttons on the version
  screen (submit → approve → publish → *edit as new draft* → assign). The prescription form is
  **driven by the chosen exercise's modality**, so "4×8 over 5 km" cannot be typed, matching the
  domain union rather than re-deciding it. Only legal moves are offered; the second-person
  approval rule is not a capability and cannot be known from capacities, so Approve is offered
  and a refusal names the reason. `add-workout` greys out days already used rather than letting
  the server reject a filled-in form.
- **Programmes (Phase 2, the competitive advantage)**: `Program → ProgramVersion → Week →
  Workout → PrescribedExercise`, lifecycle `Draft → In review → Approved → Published →
  Archived`. **A published version is immutable** — "editing" one creates a new draft and
  existing assignments keep pointing at the version they were given ([ADR-0006](docs/adr/0006-immutable-program-versioning.md)).
  Enforced **twice**: the domain refuses (enum-carried state, so "published with no
  publisher" is unrepresentable) *and* database triggers refuse, because the app is not the
  only thing that can reach those tables. Prescriptions are a modality-keyed enum, so "4×8
  reps over 5 km" cannot be written down. **`ExercisePrescription::validate()` must stay
  wired into `TemplateExercise::new`** — serde builds prescriptions field by field and skips
  the validating constructors, and without it the API accepted 99 sets. Verified by
  `scripts/verify-program-immutability.sh` (22 DB checks) and `scripts/verify-programs.sh`
  (54 HTTP checks). **No API route updates a published version, by design.**
- **Audit log**: every tenant mutation records who did what — **written in the same
  transaction as the change** (a separate write can fail alone, which is the very case an
  audit log exists for) and **append-only for the app role** (`gym_app` is denied UPDATE and
  DELETE, so tampering needs a second privileged credential). Readable only by gym managers.
- OpenAPI at `/api-docs/openapi.json`, Swagger UI at `/swagger-ui`.
- **Mobile app** (`apps/mobile`): Expo SDK 54 / RN 0.81.5 / React 19.1, expo-router with a
  **declarative `<Stack.Protected>` auth boundary**, TanStack Query (with `onlineManager` +
  `focusManager` wired to `expo-network`/`AppState` — native needs this or queries never
  refetch), Zustand, react-hook-form + Zod, FlashList, React Compiler enabled.
  Refresh token in `expo-secure-store`; access token in memory only. **Types are derived
  from the generated OpenAPI schema**, so backend contract drift fails at compile time.
  **Bundles successfully** (`expo export`) and passes `expo-doctor` 18/18.
  **Deliberately downgraded from SDK 57 on 2026-08-22** — SDK 57 required a custom
  EAS/TestFlight dev client to test on iOS (Expo Go's App Store build only ever supports
  the SDK it ships with). SDK 54 matches Expo Go's current App Store build, so the app
  installs and runs on a physical iPhone by just scanning the QR from `expo start` — no
  Apple Developer account or custom build needed. Package set re-aligned via
  `npx expo install --fix` (cross-checked against another local SDK 54 project's known-good
  pins). `@react-native-community/datetimepicker`'s optional deps pull in `react-native-windows`
  as a transitive peer, which forces `npm install --legacy-peer-deps` — upstream noise, not a
  project dependency.
- **Navigation is capacity-aware** (UX-1). Tabs come from a single data manifest,
  `apps/mobile/src/navigation/tabs.ts` — never a hand-written list in a layout. The visible
  set is derived from the capacities the signed-in account holds, so one binary is a
  member's app, a trainer's app or an owner's app depending on who signs in. Hidden tabs
  are `Tabs.Protected`-guarded (unmounted), not `href: null` (merely hidden), so "hidden"
  and "unreachable" mean the same thing. The manifest is pure, and
  `scripts/verify-nav.mjs` asserts **all 32 capacity combinations** (252 checks) with no
  device or renderer. If you add a tab, add it there and the checks come free.
- **Three handshakes removed (ADR-0031)**, because together they made the product
  impossible to finish: invitations needed an email the deployment does not send; a
  coaching pairing waited on the coach pressing accept; and a programme could reach "in
  review" in a one-owner gym with **no legal move out of it** (the second-person approval
  rule now stands down when there is nobody else who could review, and returns the moment
  there is). A member can also now put **themselves** on an offered plan, so "you cannot
  start a workout" stopped being a dead end. `verify-capacities.sh` (29),
  `verify-coaching-requests.sh` (37), `verify-programs.sh` (59).
- **Coaching relationships (Phase 2.5)**: gym-scoped, many-to-many, end-dated never deleted.
  Per-athlete authority is the **relationship**, not the trainer capacity —
  `may_view_athlete` (self/manager/coach) vs `may_coach_athlete` (self grants nothing).
  Trainers see only their clients; managers see the roster (which carries no emails).
  `scripts/verify-coaching.sh` (41).
- **Assignments**: pin a **specific published version** (never "latest"); withdrawing
  end-dates. A DB trigger re-checks version validity on insert. `scripts/verify-assignments.sh` (24).
- **Execution**: `workout_sessions → performed_sets`, prescribed and performed as separate
  immutable records; only the athlete writes their own history; performed_sets is
  INSERT+SELECT-only for the app role (privileges asserted per table). **Client-generated
  UUIDs + idempotent inserts** (the ADR-0008 primitive) — timestamps travel with the data.
  Phone logs end to end: set logger with steppers, wall-clock **rest timer** (backgrounding
  never loses time), session clock, finish/discard, "Continue workout" on Today.
  `scripts/verify-execution.sh` (39).
- **Progress is computed, never stored**: per-exercise estimated 1RM (Epley, capped at 12
  reps — the estimate picks the best set, which is not always the heaviest bar), BMI from
  height + latest weight, goal progress clamped 0..1 against a baseline captured at goal
  creation. Pure client modules under `apps/mobile/src/features/*` tested by node scripts
  (`verify-progress.mjs` 35, `verify-activity.mjs` 42, `verify-program-format.mjs` 33).
- **Profiles & body measurements**: person-owned (no `gym_id`), coach visibility via the
  relationship; measurements accept **+1 day** ("UTC+X members live tomorrow") because the
  server's "today" is UTC. `scripts/verify-profiles.sh` (38).
- **Goals**: a target on an observable metric only (bodyweight, exercise est-1RM), baseline
  captured at creation, progress computed on read, evidence-carrying status enum, uniquely
  **self-service** (members may set their own). Serde bypasses validating constructors, so
  `GoalMetric::validate()` is called at the persistence path — same pattern as
  prescriptions. `scripts/verify-goals.sh` (20). `new-goal.tsx` reads the baseline from the
  athlete's own history and shows it before committing; with no history the goal is refused
  rather than started from zero, which would read as 400% progress on day one.
  **Known gap: the API takes the baseline as INPUT**, so today the client decides it. Deriving
  it server-side from measurements/history is the stronger design and is not built.
- **Recommendations are deterministic and explainable** ([ADR-0007](docs/adr/0007-ai-authority-levels.md)
  posture): goal → programme-focus mapping + dumb substring specialty matching, every
  suggestion carries a `because`, suggestions retire when acted on, no goals → empty, never
  a guess. `scripts/verify-recommendations.sh` (15).
- **Billing (gym→member, [ADR-0010](docs/adr/0010-payments-and-billing.md))**: membership
  plans → member subscriptions → invoices → payments. **Money is minor-unit integers**, never
  floats; `format_money` lives in the domain so every surface agrees. **An issued invoice is
  immutable** — corrections are a void plus a new invoice, enforced in the lifecycle *and* by
  a trigger. **"Overdue" is derived from the due date, never stored** (a stored one needs a
  nightly job, which is a second source of truth). A subscription **copies its price at
  signup**; archiving a plan keeps existing subscribers. Payments are INSERT+SELECT only —
  a refund is a negative row — and a part-payment does not settle a bill; when one does, the
  settle happens in the **same transaction**. Managers only (`can_manage_gym`); members see
  their own. `scripts/verify-billing.sh` (49). Stripe is a seam (`PaymentProvider::Stripe`),
  not yet an integration.
- **Entitlements are resolved, never stored**: a plan declares what it `grants`; a member holds
  the union of their active subscriptions' grants; **a gym with nothing on sale bills nobody and
  withholds nothing** (`Source::NotBilled` — a named rule with a test, because getting it
  backwards stops training at every gym that uses the app without its billing). Every answer
  carries its reason, so a refusal can name the plan that would allow it instead of saying "not
  permitted". Managers bypass the gate — an owner behind on their own gym's bill must still be
  able to get in and fix it. The gate lives in `ExecutionService::start`, **not the route**, so a
  future offline replay goes through it too. No dunning yet: an owing member still trains, which
  is pinned by a test so the day suspension lands it changes on purpose.
  `scripts/verify-entitlements.sh` (35), `verify-entitlement-words.mjs` (22).
- **Navigation trades Library for Billing at manager level** — five tabs is a hard ceiling,
  so Today's quick actions carry the catalogue route for owners/admins. `verify-nav.mjs`
  asserts the trade *and* that no manager is left without a route to it.
- **Browser demo package** (`docker-compose.demo.yml`, `demo/`, [START-HERE.md](START-HERE.md)):
  the whole product for someone with **only Docker installed** — Postgres, the API, the seed
  and the app (exported by react-native-web, served by nginx) on ports 8210/8211. Written for a
  non-technical viewer: one double-click, no toolchain, no terminal.
  **`demo/share.sh` puts it on the internet** behind a free Cloudflare quick tunnel and prints
  a link — the easiest way onto **someone else's** iPhone with zero setup on their end (no
  Expo Go install, no dev machine on their network). For your own device, `expo start` +
  Expo Go now works directly: the app is on SDK 54, matching Expo Go's current App Store
  build. Three things make the tunnel/demo path work that are easy to break:
  **(1) the API is same-origin** — nginx proxies `/api`, and the web build sets
  `EXPO_PUBLIC_API_URL=""` on purpose (an empty string means "this origin"; an absolute
  `localhost:8211` is a promise about a machine we have not met, and breaks for every viewer
  but the builder). `config.ts` spells the `=== undefined` check out so a `||` refactor cannot
  silently restore the default.
  **(2) the one-tap demo buttons are `__DEV__`-gated** and an export is a *production* build,
  so `EXPO_PUBLIC_DEMO_ACCOUNTS=true` is the deliberate build-time opt-in (`SHOW_DEMO_ACCOUNTS`).
  **(3) the CORS allowlist must name PUT** — profiles and measurements are upserts and a native
  client never preflights, so the gap was invisible until a browser existed. e2e §8b pins every
  method through a preflight and that an unlisted origin still gets nothing.
- **Demo seed** (`scripts/seed-demo.sh`, idempotent): 3 accounts (owner, trainer, member —
  each holding exactly one capacity) at the one gym, published programmes (incl. an
  unassigned conditioning programme so recommendations have something to surface), a
  coaching pair, sessions with sets, measurements, goals, trainer profiles. The gym is
  seeded `is_personal=true` so the owner can both write and approve a programme version —
  there is no second catalogue-manager to hand review to. See
  [docs/test-accounts.md](docs/test-accounts.md).

**Added 2026-08-26** — four things, each with an executable suite:

- **Group classes and bookings** (`migrations/…_group_classes.sql`, `crates/domain/src/gym_class.rs`).
  A class is a **weekly slot**; a booking is a place in **one dated sitting** of it, and
  occurrences are derived in SQL (`generate_series`) rather than minted ahead — so editing
  the timetable never means rewriting unheld rows. Capacity is enforced by the domain and
  made true under concurrency by a **partial unique index**, so two people tapping Book in
  the same instant cannot both get the last place. Cancelling is a `cancelled_at` stamp,
  never a delete: that is what lets the same member re-book, and what keeps "who was in
  Tuesday's HIIT" answerable after the gym drops the class. Managers publish the timetable
  (it is gym management, not coaching — a trainer teaches a class, they do not decide the
  gym runs one); the class's **own** instructor or a manager reads the roster.
  **Booking gates on `GymAccess`, not `ClassCredits`** — the latter is still scaffolding for
  a balance that does not exist, and gating on it would refuse every member of every gym.
  `verify-classes.sh` (51), `verify-timetable.mjs` (33), 11 domain tests.
  **`chrono-tz` is a new dependency and is load-bearing**: a booking cutoff ("has 18:00
  Zumba started?") is decided on the SERVER, so the gym's IANA zone has to be resolved
  there. Comparing in UTC would be wrong for every gym not on UTC, half the year.
- **The review gate counts prescribed exercises, not weeks.** `submit_for_review` took a
  week count, so a version with one empty week — or a workout prescribing nothing — passed
  review, got **published (frozen for good)** and was assigned. The athlete then opened a
  workout on a blank screen and nobody could repair it. Two demo programmes were in exactly
  that state. The gate now counts prescriptions (`prescribed_exercise_count`), which catches
  both shapes, and the programme screen prints the reason **before** the press rather than
  after. `verify-programs.sh` (59), plus two domain regressions naming both shapes.
- **A member may put themselves on a published programme, and take themselves off it.**
  Assignment was coach-or-manager only, so somebody who joined through the open door with no
  coach could read the whole library and train against none of it — the app tracked nothing
  for them. Narrow on purpose: `athlete == actor` only, so it grants nothing over anybody
  else, and every other guard (published-only, must be a member, tenant scope) is unchanged.
  `may_coach_athlete`'s "self grants nothing" rule is **untouched** — this is a self-service
  bypass in `AssignmentService`, not a change to per-athlete coaching authority.
  `verify-assignments.sh` (30).
- **A member may end their own subscription, any time.** It was managers-only, so the product
  could *sell* a membership self-service and not stop one — a retention tactic enforced by a
  missing button (there was no client function for it at all). The money is unchanged: access
  runs to the end of the period already paid for. Cancelling is **not** leaving the gym —
  standing is untouched — so "coached → solo" is this cancel plus subscribing to a plan that
  grants `gym_access` alone, which the membership screen now offers in one step.
  `verify-billing.sh` (62).
- **`PUBLIC_BASE_URL` is now set in `.env`.** Unset, it falls back to the bound address —
  `127.0.0.1` — which is correct for a browser on the build machine and **wrong for a phone**,
  where 127.0.0.1 is the phone. Card checkout returned 201 with a `checkout_url` nothing on
  the device could reach, so "Pay" opened a dead page. Exactly the same trap as
  `EXPO_PUBLIC_API_URL`; both are now documented together in `.env.example`.

**Added 2026-08-26 (second pass)** — one ADR, three behaviour changes:

- **Who prescribes** ([ADR-0034](docs/adr/0034-who-prescribes-and-who-consents.md)).
  `AssignmentService::ensure_may_prescribe_for` replaces the old
  `ensure_may_coach`: the athlete's **own active coach**, or the athlete. A manager is no
  longer one of them. Deliberately NOT `may_coach_athlete` — that answers "may I *see* this
  athlete's data", where a manager must be able to say yes; reusing it conflated seeing with
  prescribing. `verify-assignments.sh` (31).
- **Who curates.** `can_author_programs` moves from `can_coach` to `can_manage_catalogue`,
  reversing ADR-0024's authoring half. A trainer reads the catalogue and assigns from it.
  The cost ADR-0024 missed was duplication, not risk: five trainers each writing their own
  "Beginner Strength" fragments data that is computed per exercise and per version.
  `can_propose_exercises` **stays** coach-level, with a new reason recorded in the code.
  Note authoring and publishing are now the same set, which makes the **second-person
  approval rule the only thing between one manager and the whole gym**.
  `verify-trainer-authority.sh` (36).
- **Who consents.** `POST /coach-relationships` (direct pairing) is **gone**; a manager
  calls `POST /coaching-requests/propose` and the **named trainer** accepts. New column
  `coaching_requests.raised_by`, load-bearing: for a proposal only `coach_id` may answer —
  not the proposer, not another manager — because a handshake one person can complete alone
  is decoration. A manager may not propose themselves. Member-initiated `choose` (ADR-0031)
  is untouched: granting access to *your own* data is yours to grant. The trainer's inbox is
  on Today, first, because a proposal nobody sees is how the old handshake failed.
  `verify-coaching.sh` (52, was 41).

**Added 2026-08-27** — one ADR, one feature:

- **Unplanned sessions** ([ADR-0035](docs/adr/0035-unplanned-sessions.md)). Today gains
  *Start your own workout*; `new-session.tsx` takes an optional name and a tick-list from
  the catalogue; the logging screen gains *Add an exercise* mid-session. The picked list
  is a **route parameter, not a table** — there is deliberately no "exercises I intend to
  do", because it would be a second, weaker copy of what `performed_sets` records the
  instant a set is logged.
  **Three things this uncovered rather than caused.** (1) The list and detail queries
  INNER joined `workout_templates`/`program_assignments`/`programs`; left alone, every
  unplanned session would have vanished from every history list in the product. They are
  LEFT joins now and `workout_name`/`program_name` are `Option<String>` all the way out.
  (2) `reject_finished_session_edit` compared with `<>`, which is total only while the
  columns are NOT NULL — `NULL <> 'x'` is NULL, the `IF` does not fire, and a finished
  unplanned session could have been given an assignment afterwards. Every comparison is
  `IS DISTINCT FROM` now. (3) Eight surfaces spelled the session's display name
  themselves and already disagreed ("Workout", "your workout", "Programme"); `sessionName`
  / `sessionNameFor` is the one rule, and `verify-session-name.mjs` pins that an unnamed
  own-workout and a workout whose name failed to load do **not** read the same.
  `verify-unplanned-sessions.sh` (42), `verify-session-name.mjs` (12), 8 domain tests.

**Added 2026-08-28** — one ADR, one authority bug:

- **Three capacities** ([ADR-0036](docs/adr/0036-three-capacities.md)). See decision 27.
  **Two real bugs fell out of the sweep rather than being caused by it.** The trainer
  directory read `WHERE gc.capacity IN ('trainer','head_coach')` and had **no
  `revoked_at IS NULL` filter**, so an ex-trainer stayed browsable for ever; it is now
  `= 'trainer'` and live-only. And `verify-nav.mjs` fell from 353 assertions to 119 —
  that is the combination space shrinking from 32 to 8, not coverage being dropped.
  **The suite edit was not mechanical**: where a `head_coach` was a second approver it
  became a second **owner**; where it was "staff, but not a manager, so this must be
  refused" (opening hours, timetables, plan prices, registration settings) it became a
  **trainer**. A blanket replace turned four refusal tests into permission tests that
  passed for the wrong reason — running them is what caught it.
- **The console's People page had no authority check at all.** It rendered *Add staff*
  and a per-row *Set standing* button for anybody who reached it, and called the
  manager-only roster endpoint — which 403s for a trainer, so a coach opened People to
  an error and two buttons the server would never honour. `app.tsx` gated Billing,
  Catalogue, Activity and Settings and simply left People out. Managers now get the
  roster; a trainer gets **their own clients**, built from the two things they may
  actually read (their coaching relationships and their clients' sessions), with no
  management controls — a button that exists only to produce a 403 teaches the user the
  software is broken rather than that the action is not theirs.

**On versions — read before "updating" anything:** crates are all on latest *stable*
(`argon2` stays 0.5.3 because 0.6 is a release candidate). Mobile packages are pinned by
**Expo SDK 54's `bundledNativeModules.json`, which is authoritative** — npm may show newer
(e.g. gesture-handler 3.1.0 vs SDK's ~2.28.0), but those compile into the native binary and
must match the SDK. `npx expo install --check` and `expo-doctor` are the arbiters, not
`npm outdated`. See the practices audit in [docs/tech-stack.md](docs/tech-stack.md) — its
SDK 57 figures predate the 2026-08-22 downgrade and want re-verifying next time someone is
in there.

**Two hard-won invariants — do not "simplify" these:**
1. **Refresh rotation is compare-and-swap.** `SessionRepository::revoke` returns whether *this*
   call did the revoking; losing that race is treated as theft. A read-then-write let two
   concurrent refreshes both succeed (a real bug, caught by `scripts/verify-refresh-hazard.mjs`).
2. **The mobile client de-duplicates concurrent refreshes** (`refreshOnce`). Without it, two
   simultaneous 401s rotate the token twice and sign the user out of every device.

Deliberately not built yet: the offline operation-log transport (the idempotent write path
exists; the queue does not), OpenFGA (deferred —
[ADR-0013](docs/adr/0013-mvp-application-layer-authorization.md)), push notifications (the outbox they were waiting on
now exists too — see UX-2b), a real SMTP adapter behind the `EmailSender` port, dunning
and suspension (the entitlement resolver is the one place they will change).

**CI runs on a real remote**: `origin` is github.com/meemalerk/gym-platform, and
`.github/workflows/ci.yml` runs a SUBSET of this gate on push — fmt, clippy, tests, e2e,
RLS, standing, audit, the refresh-rotation race, and the mobile typecheck/doctor/bundle.
Two things it needs that a local run does not: the e2e job must apply migrations BEFORE
`cargo build` (query! macros are checked against the live schema at compile time), and the
DB suites must reach Postgres through `scripts/lib/psql.sh` rather than by container name,
because there is no `gym-postgres` to exec into on a runner.

Person-owned tables (users, sessions,
profiles, measurements) are deliberately **outside RLS** — the service layer is the only
wall there, which is why every profile/measurement query filters by the authenticated
user id.

**Version pins worth knowing** (each cost a real bug — do not "tidy" them away):
`jsonwebtoken` needs an explicit crypto provider feature or it panics at first use;
`argon2` stays on 0.5 because 0.6 is a release candidate; `sha2` stays on 0.10 to match
argon2's `digest` ecosystem. Reasons are in [docs/tech-stack.md](docs/tech-stack.md).

## When you finish a meaningful decision or phase

- New hard choice → write an ADR (`docs/adr/`), copy the template, link it from this file's
  decisions list and from [docs/adr/README.md](docs/adr/README.md).
- Phase boundary crossed → update the "Current phase" marker in the roadmap.
- Domain term coined → add it to the glossary.
