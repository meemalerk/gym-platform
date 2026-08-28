# Roadmap

> **Current phase: Phase 4 — Monitoring, progress & background work.**
>
> **Updated 2026-08-24.** Phase 4's spine now exists: the `worker` crate, the transactional
> outbox and the periodic-job ledger ([ADR-0027](adr/0027-outbox-and-worker.md)) are built
> and draining, which unblocks reminders, coach alerts and push (UX-2b) — all three of
> which had been deferred behind it for the whole project. Also landed: trainer authority
> ([ADR-0024](adr/0024-trainer-authority.md)), coaching requests
> ([ADR-0025](adr/0025-coaching-requests.md)), open registration
> ([ADR-0026](adr/0026-open-registration.md)), a self-hosted card gateway
> ([ADR-0028](adr/0028-self-hosted-card-gateway.md)), auth hardening
> ([ADR-0029](adr/0029-auth-hardening.md)), the gym operating calendar
> ([ADR-0015](adr/0015-gym-operating-calendar.md) — designed in July, built now), the
> coach's athlete-detail screen, and `apps/console`, the instructor/admin web app
> [ADR-0009](adr/0009-client-stack.md) specified.
>
> **Still owed from Phase 3**: readiness entries, session feedback, substitutions, and the
> offline operation-log transport.
>
> Phase 2's model and API are built and verified (authoring UI still owed); Phase 2.5 is
> complete. Assignment, execution/logging (sessions, sets, timers), per-exercise progress,
> profiles, body measurements, **goals** and **deterministic recommendations** are all live
> end to end — goals and the progress/measurement surfaces were pulled forward from
> Phase 4/4.5 because the Phase 3 data they read already existed. **Still owed from
> Phase 3: readiness entries, feedback, substitutions, and the offline operation-log
> transport.** See [feature-plan-2026-07.md](feature-plan-2026-07.md) for the full build
> order. Phase 0 and Phase 1 both completed 2026-07-18. Update this marker when a phase
> boundary is crossed.

This is a capability-ordered roadmap, not a date-driven one. Each phase should leave the
product in a demonstrable, coherent state. Do not start a later phase's infrastructure
before its phase — see the non-goals in [problem.md](problem.md).

---

## Phase 0 — Planning & foundations  ✅ *complete (2026-07-18)*

Goal: agree the shape so we don't drift, and stand up the skeleton.

- [x] Problem statement, vision, non-goals
- [x] Foundational ADRs (backend language, monolith, tenancy, authz, versioning, AI
      authority, offline sync, client stack, payments, LLM, RAG)
- [x] Repo scaffolding: Cargo workspace with the crate layout from
      [architecture.md](architecture.md) (`domain`, `application`, `infrastructure`, `api`)
      — `worker` deferred until there are outbox events to drain
- [x] Postgres + migration tooling (sqlx migrate) wired
- [x] Local dev environment (docker-compose: Postgres on :5455) + README run steps
- [x] OpenAPI generation wired (utoipa) and served at `/api-docs/openapi.json`
- [ ] CI: fmt, clippy, `sqlx prepare` check, test *(carried into Phase 1)*

**Exit criteria — all verified:** `cargo test` green (32 tests), `cargo clippy -D warnings`
clean, health endpoint served by Axum (`/health` 200, `/ready` 200 with DB check), migration
`20260718000001_init` applied, OpenAPI JSON produced with both paths, Swagger UI served,
`x-request-id` propagated.

## Phase 1 — Identity, tenancy & authorization spine  ✅ *complete (2026-07-18)*

Goal: the multi-tenant skeleton with real access control before any feature is built on
sand.

- [x] Entities: users, gyms, memberships (branches, member_memberships, invitations deferred
      until there is a feature that needs them)
- [x] Tenant context plumbed through the repository layer — every tenant-owned method takes
      `(&TenantContext, id)`
- [x] Auth: sign-up, login, rotating refresh with reuse detection, logout (Argon2id; no
      password-handling shortcuts)
- [x] Authorization: `TenantScope` resolves role from the DB per request; role-gated writes.
      **OpenFGA deferred** → [ADR-0013](adr/0013-mvp-application-layer-authorization.md)
- [x] First tenant-scoped resource (exercise catalogue) proving the pattern end-to-end
- [x] `scripts/e2e.sh` — 42 assertions against a live server + database
- [x] Row-level security, **enforced at runtime**: the app connects as the unprivileged
      `gym_app` role and sets tenant context per transaction. Proven by
      `scripts/verify-rls.sh` (7 checks, including that it fails closed with no context and
      that the owner role bypasses RLS).
- [x] **Invitations** — one account gains capacities in a second gym instead of creating a
      second login. Invitations were removed in ADR-0031; standing is now proven by
      `scripts/verify-capacities.sh` (29 checks).
- [x] **Audit log** — every tenant mutation records who did what, **in the same transaction
      as the change** (a separate write can fail on its own, which is precisely the case an
      audit log exists for). **Append-only for the app role**: `gym_app` is denied UPDATE and
      DELETE, so tampering needs a second, privileged credential. Proven by
      `scripts/verify-audit.sh` (18 checks). *`domain_events` / outbox deferred to Phase 4,
      where there are consumers to drain them.*
- [x] CI (`.github/workflows/ci.yml`): fmt, clippy, `SQLX_OFFLINE` tests, live-Postgres e2e,
      refresh-race check, mobile typecheck + expo-doctor + bundle. **Written, not yet run on
      GitHub** — there is no remote configured.
- [x] Mobile app shell authenticating against this API (Expo SDK 57 — sign-up/sign-in,
      session restore, tenant-scoped catalogue read/write, types generated from OpenAPI).
      **Driven successfully in a real browser** (web target): sign-up → authenticated area →
      create exercise → list refresh, with data confirmed in Postgres.
      Still **not run on iOS/Android** (no macOS; no Android SDK on this machine).
- [x] CORS — added as an explicit allowlist after the browser run exposed that preflight
      `OPTIONS` returned 405. Node-based e2e could never catch this.

**Exit criteria — all met (2026-07-18):**
- ✅ *a user can belong to multiple gyms in multiple roles* — capacities are a set per gym,
  gained by invitation; verified end to end (one account: owner of a personal space, and
  trainer+member at another gym). **Retired 2026-08-23**: [ADR-0023](adr/0023-single-gym-deployment.md)
  caps the deployment to one gym; the capacities-as-a-set mechanism this exercised is
  unchanged, only "in multiple gyms" no longer applies.
- ✅ *an authz check correctly allows/denies cross-tenant access* — 404 (not 403) for
  non-members, capability-gated writes, **plus RLS enforced at the database layer**.
- ✅ *every mutation writes an audit entry* — atomically, and append-only.

**Still not done in this phase, deliberately:** branches, `member_memberships`, and the
outbox — none has a consumer yet. Profiles exist as tables but have no endpoints.

## Phase 2 — Exercise catalogue & programme authoring

*Starting point:* a minimal tenant-scoped `exercises` table already exists (name, modality,
notes) from Phase 1, used to prove the tenancy pattern end to end. Phase 2 grows it into the
real catalogue and then the programme model — **which is the actual competitive advantage**
([problem.md](problem.md)), so it deserves the most care.

Goal: instructors can build structured, versioned programmes. This is the heart.

- [ ] Exercise catalogue: exercises, variants, media, equipment, muscle groups, tags,
      constraints, alternatives — with gym-level governance
- [x] Programme model: programs → program_versions → weeks → workout_templates →
      template_exercises, with prescriptions as a modality-keyed enum ✅ *2026-07-18*
      *Not yet: training_blocks (no consumer until assignment exists) and progression
      rules (they belong with the adjustment engine, Phase 4).*
- [x] **Immutable version lifecycle**: Draft → In review → Approved → Published → Archived
      ✅ *2026-07-18* — enforced in the domain **and** by database triggers, because the
      application is not the only thing that can reach those tables.
      (see [domain-model.md](domain-model.md) and [ADR-0006](adr/0006-immutable-program-versioning.md))
- [x] Head-coach review/approval workflow ✅ *2026-07-18* — a staffed gym requires a second
      person to approve; a personal gym allows self-approval, since a solo coach has no
      second person and blocking them would make the feature unusable rather than safer.
- [ ] Programme authoring UI — the API is complete and usable from Swagger, but neither the
      mobile app nor a web builder can reach it yet. **This is the current gap.**
- [ ] Instructor web programme builder (spreadsheet + calendar + document hybrid)
- [ ] Richer catalogue: variants, media, equipment, muscle groups, alternatives

**Exit criteria:** an instructor authors a multi-week programme, submits for review, a head
coach approves and publishes it, and it is immutable thereafter.

## Client experience track (UX) — *runs alongside Phase 2*

Not a phase. A **parallel track** for the member/instructor app experience, worked
**one item at a time, in order**. Phases add capability; this track makes the capability
usable. Each item ships on its own and leaves the app in a working state.

Why it is ordered this way: navigation is the skeleton every later screen hangs off, so it
comes first. Polishing screens that are about to be re-parented is wasted work.

- [x] **UX-1 — Navigation shell.** ✅ *2026-07-18* Capacity-aware tab navigation. The tab set
      is derived from the capacities held **in the active gym**, so a member, a trainer and
      an owner get genuinely different apps from one binary — and switching gyms re-shapes
      the bar. Includes the gym switcher as a persistent affordance, since multi-gym
      standing is core to [ADR-0014](adr/0014-identity-capacities-and-profiles.md), not an edge case.
- [x] **UX-2 — Home ("Today").** ✅ *2026-07-19* A real landing screen. A member sees
      resume-workout, their programmes, goal progress and "Suggested for you"; a coach
      sees their clients; standing in the gym is always visible. Substance arrived with
      the Phase 3 data it reads.
- [x] **UX-2a — Activity hub.** ✅ *2026-07-18* The audit trail as something you would
      actually read: grouped by day, filterable by area, machine actions rendered as
      sentences. Date logic is pure and pinned by `scripts/verify-activity.mjs`, including
      the midnight cases that elapsed-time arithmetic gets wrong.
- [ ] **UX-2b — Push notifications.** Still not built — no device-token registration, no
      delivery, no preferences — but **no longer blocked**: the outbox exists and is
      already producing `invoice.issued`, `invoice.overdue` and `client.idle`. What remains
      is a transport and a preferences model, not infrastructure. The Activity hub is a
      *log of what has happened in a gym*, which is a different thing from telling a member
      their programme changed.
- [ ] **UX-3 — Command palette & quick actions.** Linear-style jump-anywhere search over
      gyms, exercises, people and actions. Reachable from every screen. This is what keeps
      a growing app navigable without burying things three taps deep.
- [ ] **UX-4 — Loading, optimistic & error states.** Skeletons over spinners, optimistic
      mutations with rollback, and one honest error component. Perceived speed is mostly
      this.
- [ ] **UX-5 — Accessibility pass.** WCAG 2.1 AA: contrast, 44pt touch targets, screen
      reader labels and ordering, `prefers-reduced-motion`, dynamic type. Done as its own
      item because retrofitting accessibility is far more expensive than building it in.
- [ ] **UX-6 — Copy pass.** Every label, empty state and error rewritten to say something
      true and specific. "Something went wrong" is a bug, not copy.
- [ ] **UX-7 — Light mode & theming.** The token layer already exists; this makes the
      palette switchable and honours the system setting.

**Exit criteria:** a person holding several capacities across several gyms can move through
the whole app without ever hitting a dead end, a mystery spinner or an unlabelled control.

## Phase 2.5 — Coach–athlete relationships  ✅ *complete (2026-07-18)*

Goal: the platform knows **who coaches whom**. Small, and the precondition for almost everything
requested in [feature-plan-2026-07.md](feature-plan-2026-07.md) — per-client workouts, per-client
guidance, goals, and "may this coach see this member's data".

- [x] `coach_relationships`: gym-scoped, many-to-many, lifecycle (`active` → `ended`),
      end-dated rather than deleted so history stays attributable
- [x] Domain rule: `can_coach` (gym-wide) **plus** an explicit relationship check for per-client
      data (`may_view_athlete` / `may_coach_athlete`). Not folded into capacities
- [x] "My clients" for a trainer; "My coach" for a member (People tab)
- [x] A trainer sees only their own clients; an owner sees the roster
- [x] Pairing UI (roster endpoint + person picker); roster deliberately carries no emails

**Exit criteria:** an owner assigns a trainer to a member; that trainer sees them and another
trainer in the same gym does not; ending the relationship preserves attribution of past work.

## Phase 3 — Assignment & workout execution (incl. offline)

Goal: members receive plans and execute them, offline-first.

- [x] Assignment: `program_assignments` ✅ *2026-07-18* — pins a **specific published
      version** (ADR-0006, no "assign latest"); authority is the coach *relationship*, not the
      trainer capacity; withdrawal end-dates, never deletes. `scheduled_workouts` arrive with
      execution, which is what consumes them
- [x] Execution: `workout_sessions` → `performed_sets` ✅ *2026-07-18* — prescribed and
      performed are **separate immutable records** (performed_sets: INSERT+SELECT only, revoke
      verified per table); only the athlete writes their own history; 0-rep sets are valid
      data; abandoned ≠ completed. **Client-generated ids + idempotent inserts** — the
      offline-sync primitive (ADR-0008) — and the phone logs end to end: start/continue from
      the programme view, prescription-beside-thumb set logger, finish/discard, "Continue
      workout" on Today, **rest timer** (wall-clock-anchored, so backgrounding never loses
      time; presets + "+15 s" + end buzz), **set stopwatch** for duration work, and a live
      session clock. *Still owed from this phase: readiness entries, exercise/session
      feedback, substitutions, and the offline operation-log transport itself.*
- [ ] Session feedback (exercise/session notes); readiness entries, pain/discomfort
      reporting; substitutions
- [x] Member mobile app: Today, in-session logging, rest timer, RPE/RIR ✅ *2026-07-19*
      — plus per-exercise progress (estimated-1RM trend, computed never stored),
      profiles, body measurements/BMI, goals with computed progress, and deterministic
      goal-driven recommendations (see [feature-plan-2026-07.md](feature-plan-2026-07.md)
      §4–§5). *Substitutions still owed.*
- [ ] Offline operation log + idempotent sync endpoint (see
      [ADR-0008](adr/0008-offline-sync-operation-log.md)) — the write path is already
      idempotent with client-generated ids; the transport/queue is what remains
- [ ] Domain-specific conflict resolution — design constraints, grounded in the
      research library ([research/INDEX.md](../research/INDEX.md)):
      *(a)* per-entity strategy table, not a global policy — performed sets are
      append-only facts (no conflict possible, only replay), session status is
      last-writer-wins **within the athlete's own devices**, coach-side edits never
      merge with athlete history (Preguiça 2018: convergence ≠ domain validity);
      *(b)* the client promises only what a partitioned node can honestly promise —
      sticky availability per Bailis et al. 2013, so the phone shows *its* consistent
      view and reconciles on reconnect, never pretending global state;
      *(c)* local-first ideals apply to the member's **own** training data only
      (Kleppmann et al. 2019) — tenancy, coaching authority and programme content stay
      server-authoritative, so a sync can never smuggle in an authorization decision

**Exit criteria:** a member completes a logged workout fully offline; it syncs
idempotently; published prescriptions and performed sets are preserved as separate
immutable history.

## Phase 4 — Monitoring, progress & background work

Goal: coaches see what's happening; members see progress.

- [ ] Planned-vs-performed, missed sessions, performance-drop and pain flags
- [ ] Training-load monitoring — **session-RPE-derived load** (RPE × duration) and its
      week-over-week trend as the coach-facing fatigue signal, per Halson 2014
      ([research/15](../research/15-training-load-monitoring-halson-2014.pdf)): computed
      from what members already log (ADR-0018 pattern), never a parallel data-entry
      burden. Acute:chronic-style ratios only if coaches ask; the evidence for hard
      thresholds is contested and the UI must not present one as fact
- [x] Progress: strength trends & measurements ✅ *2026-07-19, pulled forward* —
      per-exercise estimated-1RM trend (Epley, capped at 12 reps, computed client-side
      from immutable sets) and body measurements with BMI. *Volume trends, adherence and
      PR detection still owed here.*
- [ ] Worker draining the outbox: notifications, weekly summaries, analytics projections,
      coach alerts
- [ ] Communication: notes, feedback threads, announcements, session comments

**Exit criteria:** completing/ missing a workout produces the right coach alerts and
progress updates via the worker, driven off outbox events.

## Phase 5 — AI assistant with bounded authority

Goal: the talkative doorway — safe, auditable, instructor-governed.

- [ ] Orchestration layer inside the backend boundary (not a separate AI microservice)
- [ ] Intent classification → permission check → context load → approved domain tools →
      deterministic validation → persist decision + evidence → explain
- [ ] AI authority levels 0–3, configurable per gym/instructor/member (see
      [ai-authority-model.md](ai-authority-model.md), [ADR-0007](adr/0007-ai-authority-levels.md))
- [ ] Structured outputs + domain validation; approval routing for out-of-guardrail changes
- [ ] Full decision audit trail (recommendation_runs, tool_invocations, evidence,
      policy_violations)
- [ ] **EU AI Act Art. 50 compliance from day one** (applies 2026-08-02, see
      [research-2026.md](research-2026.md)): every AI-generated interaction is disclosed
      as such in the UI. The deterministic recommender (ADR-0017) is deliberately
      outside this obligation — nothing model-driven exists in the product yet, and the
      first thing that does ships with the disclosure banner, not ahead of it

**Exit criteria:** the assistant explains a plan (L0), proposes a substitution (L1), and
applies an in-guardrail session adjustment (L2) — each validated deterministically and
fully audited; out-of-bounds changes route to a coach.

## Phase 4.5 — Goals, operating calendar & nutrition

Goal: the operational surface a gym actually runs on. Sequenced here because each item needs data
or infrastructure from an earlier phase — see [feature-plan-2026-07.md](feature-plan-2026-07.md).

- [x] **Goals** ✅ *2026-07-19, pulled forward* — a target on a metric the system can already
      observe (bodyweight or an exercise's estimated 1RM), with the **baseline captured at
      creation** and progress **computed** from Phase 3 data rather than stored and drifting.
      Uniquely self-service: members may set their own goals. Evidence-carrying status enum
      (`active → achieved/abandoned`). *Adherence metrics and free-text coach notes still owed.*
- [x] **Recommendations** ✅ *2026-07-19, bonus* — deterministic, explainable: each active
      goal maps to a programme focus; published programmes with that focus and coaches whose
      stated specialties match are suggested, each with a readable `because`. No model, no
      scoring ([ADR-0007](adr/0007-ai-authority-levels.md) posture: deterministic first).
- [x] **Gym→member billing** ✅ *2026-07-25, pulled forward* — membership plans, member
      subscriptions, invoices and payments ([ADR-0010](adr/0010-payments-and-billing.md)'s
      IAP-exempt side). Money is minor-unit integers; an issued invoice is immutable
      (domain **and** trigger), "overdue" is derived from the date rather than stored, a
      subscription pins its price at signup, and payments are append-only with refunds as
      negative rows. Managers only; members see their own. `scripts/verify-billing.sh` (45).
      *Still owed: Stripe Connect money movement, dunning, tax, and the platform→gym SaaS side.*
- [ ] **Entitlement read model** (`gym_entitlements`, `member_entitlements`) — stub every
      entitlement as granted, but gate features on it **from the start**. Retrofitting feature
      gating is far more invasive than writing it in; turning on billing later then changes one
      resolver instead of every feature. Sanctioned by [subscriptions-billing.md](subscriptions-billing.md)
- [ ] **Gym operating calendar** — opening hours, closure days, special hours, trainer
      availability, and the weekly schedule **as a derived view, not a table**
      ([ADR-0015](adr/0015-gym-operating-calendar.md))
- [ ] **Reminders** — the first real consumer of the Phase 4 outbox worker. Deliberately not
      earlier: building a scheduler before the outbox means building one twice
- [ ] **Nutrition guidance** — coach-authored, versioned, attributed, requires an active coaching
      relationship. **No generated meal plans or calorie targets**
      ([ADR-0016](adr/0016-nutrition-scope.md))

## Phase 6+ — Scale & extensions (only when warranted)

Pull these forward *only* against concrete need, never speculatively:

- Classes, scheduling, facilities, equipment management
- Subscriptions / billing (RevenueCat + Stripe)
- NATS JetStream when there are independent, high-volume consumers
- ClickHouse warehouse when Postgres analytics strains
- Wearables / live-session telemetry / movement analysis
- Database-per-tenant for enterprise isolation contracts

---

## How to use this roadmap

- The **Current phase** marker at the top is the single source of truth for "what now."
- Don't build Phase N+1 infrastructure inside Phase N.
- Crossing a phase boundary: tick the exit criteria, move the marker, and note anything
  learned that should change a later phase.
