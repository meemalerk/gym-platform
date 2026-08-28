# ADR-0024: Trainer authority is three rights, not one

- **Status:** Accepted
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Refines:** [ADR-0014](0014-identity-capacities-and-profiles.md) (capacities as a set)
  and [ADR-0013](0013-mvp-application-layer-authorization.md) (application-layer
  authorization). Neither is reversed; this splits one of ADR-0013's gates.

## Context

`Capacity::Trainer` existed from Phase 1 and, in practice, could do almost nothing.

Every write worth doing was gated on `Capabilities::can_manage_catalogue()`, which is
head-coach-and-above:

| Action | Gate before this ADR |
|---|---|
| Create an exercise | `can_manage_catalogue` |
| Create a programme | `can_manage_catalogue` |
| Add weeks / workouts / prescriptions | `can_manage_catalogue` |
| Submit, approve, publish, archive | `can_manage_catalogue` |
| Assign a published version | relationship-based — **already correct** |

`can_coach()` existed but was load-bearing in exactly two places (check-in scanning and
recommendation matching). So a trainer could see their clients and start their own
workouts, and that was the job. The one thing they genuinely could do — assign a
programme to their own client — was **unreachable from the app**, because the assign
screen fetched the head-coach-only roster to populate its picker, got a 403, and rendered
the empty result as "you are not coaching anyone yet". The server was right and the
client was lying about it.

The obvious fix — "give Trainer `can_manage_catalogue`" — is wrong. It would hand every
trainer the power to publish programmes to the whole gym and to rewrite the shared
exercise catalogue, which is a real loss of control for a gym with more than two staff.

## Decision

**Split the gate along the line of whether an action binds the gym.**

Writing a draft binds nobody: it is a proposal, and the version lifecycle already has a
review step whose entire purpose is to stand between a draft and the gym's athletes.
Publishing binds every athlete subsequently put on it. That distinction — not seniority —
is the one worth encoding.

Four capability predicates, all in `Capabilities`, all answered nowhere else:

```
can_author_programs()   = can_coach()             write content, submit for review
can_publish_programs()  = can_manage_catalogue()  approve, publish, archive
can_propose_exercises() = can_coach()             name a movement
can_curate_catalogue()  = can_manage_catalogue()  promote, retire, reinstate
```

Three further rules make the split safe rather than merely wider:

1. **An author may move only their own draft.** Without this, one trainer could submit a
   colleague's half-written programme for review, and it could then be approved on its
   author's behalf. Catalogue managers are exempt — the whole catalogue is their job.

2. **The second-person approval rule is untouched.** `ApprovalPolicy::RequireSecondPerson`
   already refuses to let anyone approve a version they created. That rule is precisely
   what makes opening authoring safe: a trainer's draft cannot reach athletes without a
   head coach reading it, and a head coach's cannot without a second manager. This is not
   a capability check and must never be reimplemented as one — it cannot be answered from
   capacities, only from the version's `created_by`.

3. **A new exercise's status is decided by the constructor, from the caller's standing** —
   `Exercise::proposed_by(.., may_curate)` — never passed in as request data. A route that
   accepted `status` from the body would let a trainer post `"approved"`. This is the same
   failure mode `ExercisePrescription::validate()` exists to prevent, and it is worth
   naming twice.

### Why exercises are reviewed at all

The catalogue guard is not about tidiness or rank. **Progress is computed per
`exercise_id`** ([ADR-0018](0018-computed-progress-and-goals.md)). If "DB Bench" and
"Dumbbell Bench Press" exist as two rows, an athlete who trains both has their
estimated-1RM history silently split into two half-charts — and because `performed_sets`
reference the ids directly and are INSERT-only, no later edit can rejoin them. The damage
is permanent and invisible until someone looks at a trend and finds half of it missing.

Curation is the only thing standing between an open catalogue and that outcome, and it has
to be somebody's job.

**But review is not a gate.** A proposal is prescribable from the moment it exists
(`CatalogueStatus::is_prescribable`), so a trainer mid-programme is never blocked waiting
for a head coach to notice. The queue catches duplicates before they spread; it does not
stop work. Trading a data problem for a worse workflow problem would be a poor bargain.

### Retirement, not deletion

`retired` is the third status and it is deliberately not a `DELETE`. Performed sets
reference exercises with `ON DELETE RESTRICT`, and history does not stop being true
because the gym stopped programming the movement. A retired movement cannot be written
into a *new* prescription; existing published versions that name one are untouched, since
a published version is immutable ([ADR-0006](0006-immutable-program-versioning.md)) and
retiring a movement is not a licence to rewrite what athletes were already given.

## Consequences

**Good.** Trainer becomes a real job in the app: write a programme, name the movements in
it, send it for review, put your own clients on it once it is published. The gym keeps
control of what it publishes and what vocabulary it uses. The review step earns its
existence — it was previously a formality between two people who could both already
publish.

**Cost.** Three new states to render (`proposed` badges, a curation queue, retired
entries), one new migration, and a curation duty that did not exist before. A gym that
never works its queue accumulates proposals — they still function, so this degrades into
the previous "anyone can add anything" world rather than into breakage, which is the right
direction for a neglected chore to fail in.

**What did not change.** Assignment authority — that was already the coach–athlete
relationship, exactly as [ADR-0005](0005-relationship-based-authorization.md) intends, and
the fix there was a one-screen client bug, not a policy change. The roster stays
head-coach-only; the assign picker now reads the relationship list, which names the
athletes a trainer already has standing over, so nothing wider had to be opened to fix it.

**Verified by** `scripts/verify-trainer-authority.sh` (34 HTTP assertions, negative cases
included — an authority model that only proves what it permits has not been tested) and
the capability unit tests in `crates/domain/src/tenancy.rs`. The client mirror in
`apps/mobile/src/session/capabilities.ts` carries the same four predicates and is swept
across all 32 capacity combinations by `scripts/verify-nav.mjs`.
