# Feature plan — July 2026

> Written in response to a request for: subscription tiers, member subscriptions,
> trainer–client relations, weekly schedules, gym closure days and special opening hours,
> membership fee payments, reminders, per-client workouts, diet recommendations, goal
> setting, and member fitness tracking — "with almost perfect UX".
>
> This document is the **map**. It says what each of those is, where it already lives in the
> plan, what order things must be built in, and why. It does not restate decisions already
> recorded elsewhere; it links to them.

## The honest summary first

Of the twelve capabilities requested, **eight were already planned** and are simply not built
yet. Three are genuinely new and needed decisions of their own. One is a non-goal in disguise.

| Requested | Status before this plan | Where it lives |
|---|---|---|
| Trainer–client relations | Planned, unbuilt (`coach_athlete_relationships`) | Phase 3 |
| Per-client workouts | Planned, unbuilt (`program_assignments`, `workout_adjustments`) | Phase 3 |
| Member fitness tracking | Planned, unbuilt (`body_measurements`, `performed_sets`, PRs) | Phase 3–4 |
| Reminders | Planned, unbuilt — **blocked on the outbox worker** | Phase 4 |
| Subscription tiers (platform→gym) | **Fully decided**, deferred | [ADR-0010](adr/0010-payments-and-billing.md), [subscriptions-billing.md](subscriptions-billing.md) |
| Member subscriptions (gym→member) | **Fully decided**, deferred | same |
| Membership fee payments | **Fully decided**, deferred (Stripe Connect) | same |
| Weekly schedules | Vaguely deferred to "Phase 6+ classes/scheduling" | **New — see ADR-0015** |
| Gym closure days | Not modelled at all | **New — see ADR-0015** |
| Special opening hours | Not modelled at all | **New — see ADR-0015** |
| Goal setting | Not modelled at all | **New — this doc, §5** |
| Diet recommendations | **Absent from the product vision entirely** | **New — see ADR-0016** |

**Nothing here contradicts an existing ADR.** Where this plan moves something earlier than the
roadmap had it, that is called out explicitly with the reason.

---

## 1. The sequencing problem

These twelve things are not independent, and building them in the order they were asked for
would mean building three of them twice. The dependency graph is the real constraint:

```
coach–athlete relationship
        │
        ├──► program assignment ──► scheduled workouts ──► session logging ──► fitness tracking
        │            │                                            │
        │            └──► per-client adjustments                  └──► goals (need data to measure)
        │
        ├──► coach notes / per-client diet guidance
        │
        └──► trainer availability ──┐
                                    ├──► weekly schedule ──► reminders (needs outbox)
        gym operating calendar ─────┘
        (hours, closures, exceptions)

        entitlements ──► subscription tiers ──► member subscriptions ──► payments
        (read model)      (platform→gym)         (gym→member)             (Stripe Connect)
```

Three consequences worth stating plainly:

1. **The coach–athlete relationship is the spine.** Per-client workouts, per-client diet, coach
   notes, goals and "who may see whose data" all hang off it. It is small, and it unblocks the
   most. It goes first.
2. **Reminders cannot come early.** A reminder is a scheduled side effect, and this codebase
   deliberately has no worker yet ("no `worker` crate — it arrives when there are outbox events
   to drain", [CLAUDE.md](../CLAUDE.md)). Building reminders before the outbox means building a
   scheduler twice. Reminders stay behind the Phase 4 outbox.
3. **Payments have a business precondition, not a technical one.**
   [subscriptions-billing.md](subscriptions-billing.md) says: *"Do not build the full billing
   engine before there are paying gyms."* That is still right. But the same document sanctions
   stubbing the **entitlement read model** early, and that stub is what every feature gate in
   this plan should read. See §6.

---

## 2. Coach–athlete relationships  *(first)* ✅ *built 2026-07-18*

The missing spine. Today a gym knows who its members are and what capacities they hold, but
not **who coaches whom** — so there is no way to say "this programme is for Sara", "only
Sara's coach may read her measurements", or "show me my clients".

```
coach_athlete_relationships
  id, gym_id, coach_id, athlete_id, status, started_at, ended_at, created_by
```

Design decisions this needs, none of them obvious:

- **It is gym-scoped, not global** (`gym_id` on `coach_relationships`, matching every other
  tenant-owned table — [ADR-0004](adr/0004-postgres-shared-schema-multitenancy.md)), even
  though each deployment now serves a single gym ([ADR-0023](adr/0023-single-gym-deployment.md)).
  The original motivation — the same two people coaching in one gym and not another, since
  a trainer could work across gyms ([ADR-0014](adr/0014-identity-capacities-and-profiles.md))
  — no longer applies, but scoping by `gym_id` is the tenancy engine's normal shape, kept
  for the same reason every other table keeps it: cheap defense-in-depth, not a feature this
  specific table needed re-litigating.
- **It is a first-class record with a lifecycle, not a foreign key on a member.** Relationships
  end, and end-dating rather than deleting is what keeps a member's history attributable to the
  coach who actually wrote it. `status ∈ {active, ended}` with `ended_at`.
- **A member may have more than one coach**, and a coach obviously has many athletes. Many-to-many,
  with a partial unique index preventing two *active* rows for the same pair.
- **Authorization consequence:** `can_coach` currently answers "may this person coach *at all*
  in this gym". Per-client data needs "may this person coach *this athlete*", which is a new
  question. It goes in the domain as `Capabilities::can_coach` **plus** a relationship check —
  not folded into capacities, which are gym-wide by definition.

**Exit criteria:** an owner assigns a trainer to a member; that trainer sees the member in
"My clients"; a different trainer in the same gym does not; the relationship ends and history
remains attributed.

## 3. Per-client workouts — assignment  *(second)* ✅ *built 2026-07-18 (assignments; `scheduled_workouts` and `workout_adjustments` still owed)*

This is Phase 3 as already written, and it is what makes the programme model built in Phase 2
actually reach a person.

```
program_assignments   — athlete + program_version (a SPECIFIC version, ADR-0006) + coach + dates
scheduled_workouts    — the concrete calendar instances generated from the assignment
workout_adjustments   — per-client deviation from the template, WITHOUT editing the template
```

The important design point, and the reason ADR-0006 exists: **"a specific workout for a specific
client" must never be modelled as editing the programme.** An assignment pins a published
version; a per-client change is an *adjustment* recorded against that assignment. Two members on
the same programme with different loads is the normal case, not an exception.

## 4. Execution & fitness tracking  *(third)* ✅ *built 2026-07-18/19 (sessions, sets, body measurements, progress views; readiness and photos still owed)*

Phase 3 execution plus the health tables:

```
workout_sessions → session_exercises → performed_sets      (immutable history)
readiness_entries, body_measurements, progress_photos, injuries_and_limitations
```

**Prescribed and performed stay separate records** ([domain-model.md](domain-model.md)) — never
collapse them. "What you were told to do" and "what you did" is the entire basis of every
progress metric, adherence number and adjustment the platform will ever compute.

"Members should be able to keep track of their fitness levels" resolves to three distinct things
that are easy to conflate and should not be:

- **Performance** — derived from `performed_sets` (estimated 1RM, volume, PRs). Computed, never entered.
- **Body measurements** — entered by the member (weight, girths, photos). Self-reported.
- **Readiness** — daily, subjective (sleep, soreness, stress). Feeds the adjustment engine later.

## 5. Goals  *(fourth — needs §4's data to be meaningful)* ✅ *built 2026-07-19, plus deterministic goal-driven recommendations*

New. A goal that cannot be measured is a note, so goals are modelled as a **target on a metric
that already exists**, not free text.

```
goals
  id, gym_id, athlete_id, set_by, metric, target_value, unit, target_date,
  status ∈ {active, achieved, missed, abandoned}, achieved_at
```

- `metric` is an enum over things the system can actually observe — bodyweight, an exercise's
  estimated 1RM, sessions-per-week adherence, a measurement site. Free-text goals are allowed
  **only** as a coach note, deliberately in a different table, so nothing pretends to track them.
- Progress is **computed** from §4's data, never stored as a duplicate number that can drift.
- Achievement is evaluated when new data arrives, not on a timer — a goal reached at 07:00 should
  say so at 07:00, and a nightly job would be both later and another thing to run.

## 6. Subscriptions, tiers and payments  *(fifth — see the caveat)*  — **built**

**Already fully decided.** Read [ADR-0010](adr/0010-payments-and-billing.md) and
[subscriptions-billing.md](subscriptions-billing.md) before touching any of this; the table
names, the Stripe Connect model, the IAP-exemption reasoning and the tax approach are all
settled. Nothing in this plan changes them.

What this plan *does* change is a small piece of ordering:

> **Build `member_entitlements` and `gym_entitlements` as read models early, and gate every
> feature on them from the start — even while every entitlement is granted unconditionally.**

The reason: retrofitting feature gating is far more invasive than writing it in. If the code
already asks "is this gym entitled to X?" and the answer is currently always yes, turning on
real billing later is a change to *one* resolver. If it does not ask, turning on billing means
touching every feature. [subscriptions-billing.md](subscriptions-billing.md) already sanctions
exactly this ("should be stubbed earlier so feature-gating … has something to read").

**How it landed.** The read model is `EntitlementService` (`crates/application/src/entitlements.rs`),
resolving on read rather than storing rows — a stored "entitled until the 30th" is wrong on the
31st unless a nightly job rewrites it, which is the second source of truth this codebase refuses
for "overdue". A plan declares what it `grants`; a member holds the union of their active
subscriptions' grants; **a gym with nothing on sale bills nobody and withholds nothing**
(`Source::NotBilled` — a named rule with a test, not an implicit fallthrough, because getting it
backwards would stop training at every gym that uses the app without its billing). Every answer
carries the reason it was reached, so a refusal names the plan that would have allowed it.
Managers bypass the gate: an owner behind on their own gym's bill still has to be able to get in
and fix it.

`GET /api/v1/gyms/{gym_id}/entitlements/me` exposes it; `ExecutionService::start` is the first
caller — asked in the service, not the route, so a future offline replay goes through the same
gate. `scripts/verify-entitlements.sh` (35) and `scripts/verify-entitlement-words.mjs` (22)
pin it.

The full billing engine — Stripe Connect onboarding, invoices, webhooks, dunning, tax — stays
where it is. The existing guidance is right and this plan does not overrule it:

> *"Do not build the full billing engine before there are paying gyms."*

**Membership fee payments specifically** are gym→member money movement via Stripe Connect. That
is the largest single piece of work in this document — KYC onboarding, `application_fee_amount`,
webhook reconciliation into `provider_events`, refunds, failed-payment handling — and it is
worth doing once, properly, when there is a gym waiting to be paid.

## 7. Gym operating calendar  *(sixth — see [ADR-0015](adr/0015-gym-operating-calendar.md))*

"Weekly schedules, gym close days, special opening hours" is one problem wearing three hats:
**when is this gym open, and what is on**. Modelled together or it will be modelled three times.

```
gym_opening_hours     — the recurring weekly pattern (per branch, later)
gym_calendar_overrides — a specific date that differs: closed, or different hours
trainer_availability   — the same shape, for a person
```

The hard part is not the tables, it is the resolution rule and timezones. See the ADR.

## 8. Diet & nutrition  *(last — see [ADR-0016](adr/0016-nutrition-scope.md))*

This one needs a decision before it needs a schema, because it is the only item on the list that
touches an existing **safety** boundary: [problem.md](problem.md) prohibits *"personalized
medical … advice generated by the AI without instructor authority."*

Nutrition is also entirely absent from the product vision — it is not a deferred feature, it is a
new product surface. ADR-0016 decides what we will and will not build. In short: **coach-authored
nutrition guidance, yes; automated diet prescription, no; anything resembling clinical dietetics,
no.**

---

## 9. What "almost perfect UX" means here

Not a separate phase. Applied to each feature as it lands, and measured against the existing UX
track ([roadmap.md](roadmap.md)). The rules that matter most for this batch:

- **Every new surface is capacity-aware from the first commit.** The navigation manifest
  (`apps/mobile/src/navigation/tabs.ts`) already derives what a person can see from what they may
  do; new screens join it rather than working around it.
- **No screen shows a number it cannot explain.** A goal shows the data behind it; adherence shows
  which sessions counted. This is the difference between a dashboard and a fortune teller.
- **Nothing fabricates data to look complete.** If the endpoint does not exist, the screen says so.
  (Precedent: the People tab honestly scopes itself to invitations because there is no roster
  endpoint yet.)
- **Every destructive or money-moving action states its consequence before it happens** — ending a
  coaching relationship, cancelling a membership, charging a card.
- **Accessibility is written in, not retrofitted** (UX-5). Retrofitting it is how it never happens.

## 10. Build order

Each item is a phase boundary; each leaves the app working and is verified before the next starts.

0. ✅ **Programme authoring UI** — Phase 2's owed half. Create → weeks → workouts →
   prescriptions → the lifecycle as buttons → assign. Out of numbered order because it was
   owed from an earlier phase, not new scope.
1. ✅ **Coach–athlete relationships** — the spine. Unblocks the most, smallest of the group.
2. ✅ **Programme assignment** — makes Phase 2's programmes reach a person. *(Per-client
   adjustments still owed.)*
3. ✅ **Execution & fitness tracking** — logging, measurements, progress trends. The data
   everything else reads. *(Readiness, photos, PR detection still owed.)*
4. ✅ **Goals** — meaningful only once (3) exists. *(Landed with recommendations riding on it.)*
5. ✅ **Entitlement read model** — small, and unblocks correct feature gating everywhere.
   *(Plans declare `grants`; `EntitlementService` resolves them on read; starting a session is
   the first gated surface. Suspension and dunning still owed — an owing member still trains,
   deliberately and with a pinned test.)*
6. **Gym operating calendar** — hours, closures, weekly schedule.
   **← next when development resumes**
7. **Outbox worker → reminders** — the worker first, reminders as its first real consumer.
8. **Nutrition guidance** — per ADR-0016, coach-authored only.
9. **Billing engine** — when there are paying gyms. Per ADR-0010, unchanged.

**One at a time.** Not because the work is hard individually, but because each of these has a
schema, an authorization story, a test suite and a UI, and half-finishing four of them produces
a codebase where nothing can be trusted.

## 11. Known gaps, stated rather than forgotten

Small, real, and each one a deliberate not-yet rather than an oversight:

- **A goal's baseline is client-supplied.** `CreateGoalRequest` takes the whole `GoalMetric`,
  baseline included, so the phone decides where progress is measured from. `new-goal.tsx` reads
  it honestly from the athlete's own measurements or lift history — but nothing stops a
  different client sending a flattering number. The fix is to take only the *target* over the
  wire and derive the baseline in `GoalService` from the same data the progress calculation
  reads. Contained: one request type, one service, `verify-goals.sh`.
- **A coach cannot yet open a client's profile or measurements from the app.** The endpoints
  exist and are gated correctly (`getAthleteProfileOf`, `getMeasurementsOf` are wired in the
  API client); no screen calls them. The natural home is a member detail screen off People.
- **Ending a coaching relationship has no button.** `endCoaching` exists in the client;
  `assign-coach.tsx` only pairs. Pairing without unpairing is a one-way door.
- **Payments are recorded but never listed per invoice.** `listPayments` is wired; the invoice
  row settles in full or voids. A part-payment history is visible only in the audit trail.
- **No dunning.** An owing member still trains — pinned by a test in `verify-entitlements.sh`
  so the day suspension lands, it changes on purpose.
