# Product Specification

> The single document that says **what this product is, who it is for, what it does, and
> what it refuses to do**. Where this disagrees with an ADR, the ADR wins on *how* and this
> document wins on *what*. Where it disagrees with the code, the code is either a bug or this
> document is stale — say which.
>
> Companion documents: [problem.md](problem.md) (why), [domain-model.md](domain-model.md)
> (the nouns), [roadmap.md](roadmap.md) (when), [adr/](adr/) (the hard choices and their
> reasoning).

---

## 1. In one paragraph

A single-gym platform for a gym that **coaches people**, not one that sells door access. A gym
writes training programmes, versions them, reviews them, publishes them and assigns them to
members; members execute those programmes and the system keeps their training history; coaches
read that history and adjust. Billing, entitlements and an audit trail sit underneath so the
business can actually be run on it. The competitive advantage is the programme model, the
instructor workflow and the longitudinal training data — not a chatbot.

Built on multi-tenant-*capable* infrastructure ([ADR-0004](adr/0004-postgres-shared-schema-multitenancy.md)) —
`gym_id`-scoped tables, row-level security — but each deployment is capped to exactly one gym
([ADR-0023](adr/0023-single-gym-deployment.md)). The isolation machinery stays on as
defense-in-depth; there is simply nothing else to isolate from.

## 2. Who it is for

| User | What they come for | What would make them leave |
|------|--------------------|----------------------------|
| **Gym owner** | Run the business: staff, members, money, and evidence of what happened | Money that does not reconcile; no way to see who changed what |
| **Administrator** | Day-to-day operations without owning the place | Being unable to fix things without the owner |
| **Head coach** | Programme quality across the floor; review and sign-off | Having to trust that a trainer's programme is safe with no review step |
| **Trainer** | Their own clients, their own programmes, their own accountability | Seeing the whole gym's roster, or being seen by clients they do not coach |
| **Member** | Know what to do today, log it, see it add up | Losing a set because the app was offline; being shown someone else's data |
| **Solo coach / self-coached athlete** | The same tools without inventing an organisation | Being forced through gym onboarding to log a squat |

**Not for**: casual step-counting, social fitness feeds, or nutrition-led weight loss. Those are
different products with different regulatory shapes.

## 3. The tenancy model

```
The (one) Gym  ──────────────────────── everything below is owned by it
  ├── People, each holding a SET of capacities in this gym
  │     owner · trainer · member
  ├── Exercise catalogue
  ├── Programmes → versions → weeks → workouts → prescriptions
  ├── Coaching relationships (who may see and direct whom)
  ├── Assignments (which member is on which programme VERSION)
  ├── Membership plans → subscriptions → invoices → payments
  └── Audit log
```

**One account, capacities as a set** ([ADR-0014](adr/0014-identity-capacities-and-profiles.md)).
A person is not "a trainer"; they *hold trainer in this gym*. The gym can have several owners.
Someone can be a trainer *and* a member at once — a common real arrangement that a single-role
model cannot express.

**Person-owned vs gym-owned.** Profiles and body measurements are person-owned, carrying no
`gym_id` — a holdover from when an account could belong to several gyms and needed data that
followed it between them ([ADR-0014](adr/0014-identity-capacities-and-profiles.md)). Everything
else belongs to the gym. This split is still deliberate: your height is not the gym's property,
but the programme they wrote is.

**One gym, period** ([ADR-0023](adr/0023-single-gym-deployment.md)). Earlier revisions of this
document described a platform of many gyms with a switcher between them, and a "personal gym" for
solo users. That choice is superseded: `gyms` is capped to a single row per deployment. A solo
coach or self-coached athlete still gets the same tools — they are simply this deployment's
owner, not a special case.

## 4. What the product does

### 4.1 Identity and access
- Sign-up provisions a gym, an owner and a membership atomically.
- Invitations are per-email, single-use, hash-stored, and bound to the invited address, so a
  forwarded link is useless. Only owners may invite owners.
- Accepting an invitation adds capacities to the **existing** account rather than creating a
  second one.
- Holding no capacity in a gym yields **404, not 403** — tenant existence is not leaked.

### 4.2 Exercise catalogue
- Gym-scoped. Every exercise declares **how it is measured**: repetitions, duration, or distance.
- Reading is open to everyone in the gym; writing is capability-gated.
- The modality is load-bearing, not metadata — it decides what may be prescribed.

### 4.3 Programmes — the core
- `Programme → Version → Week → Workout → PrescribedExercise`.
- Lifecycle: **Draft → In review → Approved → Published → Archived**.
- **A published version is immutable.** "Editing" one creates a new draft; existing assignments
  keep pointing at the version they were given ([ADR-0006](adr/0006-immutable-program-versioning.md)).
- **Nobody approves their own version** in a gym with more than one person in it. A personal gym
  self-approves, because there is no second person to ask.
- A prescription is a **modality-keyed union**: "4×8 reps over 5 km" cannot be written down, in
  the UI or in the database.

### 4.4 Coaching relationships
- Gym-scoped, many-to-many, **end-dated and never deleted**.
- Per-athlete authority is the *relationship*, not the trainer capacity. `may_view_athlete`
  (self / manager / their coach) is separate from `may_coach_athlete` (self grants nothing —
  you do not direct your own programming).
- Trainers see only their own clients. Managers see the roster, which carries no email addresses.

### 4.5 Assignment and execution
- An assignment pins a **specific published version**, never "latest".
- Only the athlete writes their own history. A coach decides what you *should* do; only you can
  say what you *did*.
- Sessions and performed sets are separate immutable records. Performed sets are
  insert-and-read only — there is no update path.
- Ids are minted **on the device** and inserts are idempotent, so a phone can retry a sync
  forever without duplicating history ([ADR-0008](adr/0008-offline-sync-operation-log.md)).

### 4.6 Progress and goals
- Progress is **computed from immutable history, never stored**: estimated 1RM (Epley, capped at
  12 reps), BMI, goal progress clamped 0..1 ([ADR-0018](adr/0018-computed-progress-and-goals.md)).
- A goal targets an **observable** metric only — bodyweight, or a lift's estimated 1RM. "Get
  fitter" is not a goal the system can measure, so it is not offered.
- Goals are the one thing a member sets for themselves.

### 4.7 Recommendations
- **Deterministic rules with readable reasons** ([ADR-0017](adr/0017-deterministic-recommendations.md)).
  Goal → programme-focus mapping, plus substring specialty matching for trainers.
- Every suggestion carries a `because`. No goals means no suggestions — never a guess.

### 4.8 Billing and entitlements
- Membership plans → subscriptions → invoices → payments. Money is **minor-unit integers**.
- An **issued invoice is immutable**: a correction is a void plus a new invoice; a refund is a
  negative payment. Neither is an edit.
- "Overdue" is derived from the due date on read, never stored.
- A subscription **copies its price at signup**; archiving a plan keeps its subscribers.
- **Entitlements are resolved on read, never stored.** A plan declares what it `grants`; a member
  holds the union of their active subscriptions' grants.
- **A gym with nothing on sale bills nobody and withholds nothing.** Absence of billing is not
  absence of entitlement.
- Managers bypass the gate — an owner behind on their own gym's bill must still be able to get in
  and fix it.

### 4.9 Audit
- Every tenant mutation records who did what, **written in the same transaction as the change**.
- **Append-only for the application role**: tampering requires a second, privileged credential.
- Readable only by gym managers.

## 5. What the product deliberately does not do

Each of these is a decision, not a backlog item.

| Not built | Why |
|-----------|-----|
| **AI-generated programmes or meal plans** | The advantage is the deterministic programme model. An LLM in the prescription path makes the one thing that must be reviewable unreviewable. |
| **Nutrition prescription** | Coach-authored guidance only ([ADR-0016](adr/0016-nutrition-scope.md)). Calorie targets keyed on a medical condition carry real regulatory and human risk for no differentiation. |
| **Learned ranking / recommendation ML** | A suggestion that cannot explain itself cannot be argued with, and a coach must be able to argue with it. |
| **Social feed, likes, challenges** | A different product. It would also make training data social, which changes its privacy shape entirely. |
| **Door hardware / turnstile integration** | Real demand, but it is an integration business, not this one. |
| **Dunning and suspension** | Deliberately absent *for now* — an owing member still trains, pinned by a test so the day it changes, it changes on purpose. |

## 6. Rules that must never quietly change

These are the invariants. Breaking one is a product change, not a refactor.

1. A published programme version is immutable.
2. Only the athlete writes their own training history.
3. Tenant isolation holds even if the application layer is wrong — RLS is the second wall.
4. The audit log cannot be edited by the application role.
5. An issued invoice is never rewritten.
6. Progress and entitlement are computed, never stored.
7. A gym that does not bill through the platform withholds nothing.
8. No permission question is answered by inspecting a raw capacity set — it goes through
   `Capabilities::can_*`.

## 7. Quality bar

- **Verification-first** ([ADR-0019](adr/0019-verification-first-development.md)): every feature
  ships an executable check against a live server and database. Bugs become pinned regressions.
- **Accessibility is written in**: WCAG AA contrast is asserted by script across both colour
  schemes; every interactive element carries a label.
- **One focal element per screen** ([ADR-0020](adr/0020-design-system.md)). If everything is
  emphasised, nothing is.
- **Invalid states unrepresentable**: enums over nullable-field bags, in Rust and in the wire
  format.

## 8. Success measures

What would tell us this is working, in rough order of honesty:

1. A gym writes a programme, publishes it, and assigns it **without being shown how**.
2. Members log sessions on days nobody reminded them to.
3. A coach changes a programme *because of* something they saw in the history.
4. The audit log is opened during a real disagreement, and settles it.
5. A gym's billing reconciles against their bank without a spreadsheet.

## 9. Current state

See the "Current phase" marker in [roadmap.md](roadmap.md) and the status section in
[CLAUDE.md](../CLAUDE.md); both are kept current and this section deliberately does not
duplicate them.
