# Project log

*A dated record of how this project was actually produced: what was built in what order,
what the tooling was, what went wrong, and what evidence exists for each claim. The git
history is the primary record (42 commits; subjects quoted below are verbatim); this
document is the narrative index over it.*

---

## Statement on the use of generative AI

This project was developed with substantial, **declared** use of a generative AI coding
assistant (Anthropic's Claude, used through the Claude Code development tool), in line
with UWE Bristol's published principles on generative AI — which permit AI use where the
assessment allows it and require that AI-assisted work be acknowledged, and which state
that submitting AI-created work *without* acknowledgement is an assessment offence
(UWE Bristol, 2026a; 2026b).

**Division of labour.** The working model throughout was: the author set the product
direction, requirements and priorities, made or ratified every recorded architecture
decision (the ADRs in [docs/adr/](../adr/) each name their decider), reviewed the output,
and directed the corrections; the assistant carried out implementation, drafted
documentation, and proposed designs for ratification. Every commit produced with the
assistant carries a `Co-Authored-By` trailer identifying it — the attribution is in the
permanent record, deliberately.

**Why the methodology is defensible.** Because implementation was fast, the project's
discipline moved to *verification*: no feature counts as done until an executable check
against the live server and database says so ([ADR-0019](../adr/0019-verification-first-development.md)).
The result is 16 verification suites (~1,000 assertions) that a marker can run
independently. The defects table below records what those checks caught — including
defects in the assistant's own output, which is precisely why the verification-first
rule exists.

**Data protection.** No personal or sensitive data was entered into the AI tool; all
demo data is synthetic (`*@demo.test` accounts, seeded by script), consistent with UWE's
guidance on AI tools and personal data (UWE Bristol, 2026b).

*References for this statement:* UWE Bristol (2026a) *Principles for using generative
artificial intelligence (AI)*; UWE Bristol (2026b) *Using generative AI at UWE Bristol*;
full citations in [assignment-report.md](assignment-report.md).

---

## Timeline

### 2026-07-18 — Foundations to coaching (26 commits)

**Phase 0 — planning and skeleton.** Problem statement, sixteen planning documents,
thirteen foundational ADRs, Cargo workspace (`domain` / `application` /
`infrastructure` / `api`), Postgres 17 via docker-compose, migrations, OpenAPI
generation, health endpoints. *"Phase 0: planning docs + verified Rust backend
foundation."*

**Phase 1 — identity, tenancy, authorization.** Argon2id auth with rotating refresh
tokens and family-revocation on reuse; the `TenantScope` extractor (capacities re-read
from the DB per request; absence → 404, not 403); the exercise catalogue as the pattern
proof; **row-level security made real at runtime** (non-owner role, transaction-scoped
context — *"Make RLS actually enforce at runtime"*); invitations (email-bound,
single-use, hash-stored, uniform 404s); the append-only, same-transaction audit log.
Identity was reworked mid-phase into **one account, many capacities**
([ADR-0014](../adr/0014-identity-capacities-and-profiles.md)) when the one-role-per-gym
schema proved unable to express a trainer who also trains.

**Mobile.** Expo SDK 57 app with a declarative auth boundary, generated OpenAPI types,
session restore, then: onboarding split (account first, intent second), gym switcher,
capacity-aware navigation manifest (UX-1), a design pass (depth, motion, haptics, safe
areas), and the Activity hub. Verified in a real browser and on a physical phone over
LAN — the browser run immediately exposed a missing CORS preflight (*"Run the app in a
real browser; add CORS allowlist and CI"*), which no Node-based test could have caught.

**Phase 2 — the programme model.** `Program → ProgramVersion → Week → Workout →
PrescribedExercise` with evidence-carrying lifecycle enums; immutability enforced in the
domain *and* by database triggers; modality-keyed prescriptions; review/approval
workflow. *"Phase 2: the programme model — domain and storage"* and *"…authoring is
reachable — repository, service, HTTP."*

**Feature plan.** The full requested capability set (subscriptions, schedules,
relations, goals, tracking, nutrition…) was mapped into a dependency-ordered plan
([feature-plan-2026-07.md](../feature-plan-2026-07.md)) with two new ADRs (operating
calendar; nutrition scope) *before* implementation continued — *"docs: plan the
requested feature set, with two new ADRs."*

**Phase 2.5 — coaching relationships.** The spine: gym-scoped, many-to-many, end-dated
never deleted; `may_view_athlete` / `may_coach_athlete` as domain functions; People tab,
roster endpoint (no emails), pairing UI; idempotent demo seed.

### 2026-07-19 — Assignment to recommendations (16 commits)

| Slice | Commit subject (verbatim) | Evidence added |
|---|---|---|
| Assignment | *"Phase 3: programme assignment — a published version reaches a person"* | `verify-assignments.sh` (24) |
| Version view | *"The programme card opens the plan: version-content view"* | — |
| Execution | *"Phase 3: workout execution — sessions and performed sets"* | `verify-execution.sh` (39) |
| Demo seed | *"seed: a demo gym that looks lived-in"* | idempotency re-run check |
| Logging UI | *"The phone can log a workout: session screen, start/continue, Today card"* | — |
| Timers | *"Timers: rest between sets, a stopwatch for holds, a session clock"* | `verify-format-clock` (33) |
| Progress | *"Progress: per-exercise history, and the chart that shows it"* | `verify-progress.mjs` (35) |
| Profiles | *"Profiles: the person behind the account, at last"* | `verify-profiles.sh` (38) |
| Body tracking | *"Body tracking: weight, BMI, body fat, girths — and two bugs of note"* | (same suite) |
| Inputs | *"Inputs that respect thumbs: date picker, unit fields, steppers"* | — |
| Goals | *"Goals: measurable targets with live progress, seeded"* | `verify-goals.sh` (20) |
| Recommendations | *"Recommendations: goals suggest programmes and coaches, with reasons"* | `verify-recommendations.sh` (15) |

Then development was **paused** for the documentation package: the screenshot corpus
(44 captures via a headless-browser harness driving the real UI), the research library
(16 verified open-access PDFs, [research/INDEX.md](../../research/INDEX.md)), this log, and
the assignment report — plus three retrospective ADRs (0017–0019) recording decisions
that had been made in code but not yet on paper.

---

## Defects found and pinned (selected)

Each of these was found by a verification script or a real-device run, fixed, and left
behind as a regression assertion — the project's core quality mechanism
([ADR-0019](../adr/0019-verification-first-development.md)).

| Defect | Where it hid | Lesson pinned by |
|---|---|---|
| Two concurrent token refreshes both succeeded | Read-then-write revocation in SQL | Race harness `verify-refresh-hazard.mjs`; rotation is now compare-and-swap |
| API accepted a 99-set prescription | serde builds values field-by-field, bypassing validating constructors | Validation wired into the sole persistence path; e2e asserts the 422 |
| Negative SQL assertions passed vacuously | `psql` exits 0 on SQL error by default | `ON_ERROR_STOP=1` in every SQL suite; the suite fails if a negative test cannot fail |
| App role silently held DELETE/UPDATE on new tables | Migration 0003's `ALTER DEFAULT PRIVILEGES` made narrow grants additive | Privilege matrix asserted per table in `verify-execution.sh` |
| Trigger referenced a column of the wrong table | PL/pgSQL `CASE` resolves all branches' column refs | Rewritten with `IF/ELSIF`; immutability suite covers the trigger |
| Same-day measurements rejected for UTC+X members | Server "today" is UTC; members east of Greenwich live in tomorrow | Domain tolerance (+1 day) with a test named for the lesson |
| Serialized enum tag mangled (`est1_rm`) | serde snake_case digit handling | Explicit `#[serde(rename)]` + a domain test pinning the wire tag |
| "Best set" chosen by heaviest bar | Author's own test encoded the wrong intuition | Test rewritten: best set is by *estimate* (5×65 beats 3×67.5 under Epley) |
| Dev-server kill script terminated its own caller | `pgrep -f` matched the calling shell's command string | `pgrep -x` + working-directory guard |
| Missing CORS preflight (405) | Only observable from a real browser | Browser-driven check; explicit allowlist |

## Verification inventory

`scripts/all-check.sh` runs every suite: 163 workspace unit tests; 49 end-to-end HTTP
assertions (pinning the OpenAPI path count, 39); and per-feature suites — RLS 7,
invitations 22, audit 18, programme immutability 22 (DB), programme authoring 55 (HTTP),
coaching 41, assignments 24, execution 39, profiles/measurements 38, goals 20,
recommendations 15, navigation 252 (all 32 capacity combinations), activity 42,
progress 35, formatting/timer 33. TypeScript strict compile and `expo-doctor` 20/20
gate the mobile app; `cargo clippy -D warnings` gates the backend.
