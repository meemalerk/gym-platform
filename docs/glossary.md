# Glossary

Use these terms exactly, in code and in conversation. Consistent vocabulary keeps the
domain model coherent across sessions. Add new terms here when you coin them.

## Tenancy & organisation

- **Gym** — the tenant. The unit of isolation; `gym_id` scopes tenant-owned data. Each
  deployment serves exactly **one** (ADR-0023, single-gym deployment) — the tenancy engine
  underneath stays multi-gym-*capable* (ADR-0004), but the product no longer offers a way
  to create or join a second one. A **personal gym** (`is_personal`) — a solo user's own
  workspace, distinct from a commercial one — is vestigial post-ADR-0023: there is only
  ever the one gym, so nothing for a personal one to be distinct *from*.
- **Account** — one per person, platform-wide. A person never has a second login (ADR-0014).
- **Capacity** — what a person may do *in one gym*. A **set**, not a single role:
  `owner`, `trainer`, `member` (ADR-0036 removed `admin` and `head_coach`). Replaces
  the old single "role". The set is what makes "a trainer who also trains here" and
  "an owner who coaches" expressible — `trainer` means *coaching*, not seniority.
- **Capabilities** — the resolved set of capacities, and the *only* place permission questions
  are answered (`can_manage_catalogue`, `can_coach`, …).
- **Profile** — personal data describing someone in a capacity: **athlete profile** (goals,
  training age, limitations) and **trainer profile** (bio, certifications). Person-owned, no
  `gym_id`. There is deliberately **no "owner profile"** — ownership is a capacity, not data.
- **Branch** — a physical/organisational location within a gym. *(Not yet built.)*
- **Owner** — runs a gym: standing, settings, billing, the catalogue, publishing
  programmes. The only rung that manages anything since ADR-0036.
- **Trainer** — coaches: reads the catalogue, assigns published programmes to their own
  clients, proposes exercises. *(Renamed from "instructor" in ADR-0014 to match the
  language owners actually use.)* Does **not** run the gym.
- **Member** — trains; uses the mobile app; executes assigned programmes.
- **Athlete** — a member in the context of a coaching relationship (the coached party).

## Coaching

- **Coach–athlete relationship** — the link that makes an instructor a coach of a specific
  member; drives visibility.
- **Coach note** — versioned note from a coach; edit history preserved.
- **Consultation** — a scheduled coaching interaction.
- **Nutrition guidance** — a versioned document a coach writes for one athlete. Coach-authored
  only; the platform never generates dietary prescription ([ADR-0016](adr/0016-nutrition-scope.md)).
- **Goal** — a target on a metric the system can already observe (bodyweight, estimated 1RM,
  adherence), with progress **computed** rather than stored. An aspiration that cannot be
  measured is a *coach note*, not a goal.

## Gym operations

- **Opening hours** — the recurring weekly pattern for when a gym is open. Stored as wall-clock
  `TIME` in the gym's IANA timezone, never as an absolute instant ([ADR-0015](adr/0015-gym-operating-calendar.md)).
- **Calendar override** — a specific date that differs from the pattern: closed, or different
  hours. An override wins over the pattern entirely; there is no merging.
- **Trainer availability** — the same shape as opening hours, for a person. Bookable time is the
  intersection of the two.
- **Weekly schedule** — what is on this week. A **derived view**, never a stored table, so it
  cannot disagree with the opening hours and overrides it is resolved from.
- **Entitlement** — the resolved answer to "what may this actor access right now", decoupled from
  where payment came from. Feature gating reads entitlements, **never** payment-provider state
  ([subscriptions-billing.md](subscriptions-billing.md)).

## Programming

- **Exercise** — an entry in the catalogue; may have **variants** and **media**.
- **Exercise constraint** — a limitation/rule on an exercise (e.g. contraindications).
- **Exercise alternative** — an approved substitute for an exercise (the pool the AI may
  draw from at Level 2).
- **Program** — a named training programme; a container for versions.
- **Program version** — an immutable-once-published snapshot of a program's content.
- **Training block / Mesocycle** — a multi-week structural unit within a version.
- **Program week** — a week within a block.
- **Workout template** — the planned session definition within a week.
- **Prescription** — what to do for an exercise (reps/duration/distance variants; see
  [domain-model.md](domain-model.md)).
- **Prescription rule / Progression rule** — how prescriptions evolve (progression,
  deload, conditional alternatives).

## Program lifecycle states

- **Draft** → **In review** → **Approved** → **Published** → **Archived**.
- **Published** — immutable. Editing produces a new **draft** version.

## Assignment & execution

- **Program assignment** — links a program version to a member (individual or group).
- **Scheduled workout** — a planned instance of a workout template on a date for a member.
- **Workout adjustment** — a modification to a scheduled workout (may originate from AI or
  coach).
- **Coach approval** — the review gate for adjustments/changes that require a human.
- **Workout session** — a member's actual execution of a scheduled workout.
- **Unplanned session** — a workout session with **no** assignment and no workout
  template: one the member built themselves, because nobody prescribed them anything
  (ADR-0035). The shape an Open Gym member trains in. Say *unplanned*, never "open" —
  an *open* session is one still in progress, whichever kind it is.
- **Session exercise / Performed set** — immutable records of what was actually done.
- **Readiness entry** — a member's pre-session readiness/recovery check.
- **RPE / RIR** — Rate of Perceived Exertion / Reps In Reserve; intensity feedback.
- **PR (Personal Record)** — a best performance achievement.

## AI

- **Assistant authority** — the per-gym/instructor/member policy defining what the AI may
  do (levels 0–3). See [ai-authority-model.md](ai-authority-model.md).
- **Recommendation run** — one audited AI decision episode.
- **Tool invocation** — a single call the AI made to an approved domain tool.
- **Recommendation evidence** — the context/data a recommendation was based on.
- **Policy violation** — a recorded instance where a proposal breached guardrails (and was
  routed to a human or rejected).

## Platform mechanics

- **Tenant context** — the `gym_id` (and actor) that every repository call must carry.
- **Transactional outbox** — state change + event written in one transaction; drained by
  the worker.
- **Domain event** — a fact about something that happened (e.g. `WorkoutCompleted`).
- **Operation log** — the offline client's ordered list of operations to sync idempotently.
- **ReBAC** — relationship-based access control (OpenFGA); see
  [authorization-model.md](authorization-model.md).
- **Domain policy** — deterministic rules answering "is this action valid?" (distinct from
  authorization).
- **ADR** — Architecture Decision Record; see [adr/](adr/).
