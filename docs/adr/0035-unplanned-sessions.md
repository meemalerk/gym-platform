# ADR-0035: Unplanned sessions — training with nobody prescribing

- **Status:** Accepted
- **Date:** 2026-08-27
- **Deciders:** Project author

## Context

A member selects the **Open Gym** membership. It grants `gym_access` and
nothing else — no `coached_programming` — which is the honest description of
what most gyms sell most of: use the floor, on your own, whenever you like.

That member could not record a single thing.

The chain is short and every link was deliberate. `gym_access` without
`coached_programming` means no coach. ADR-0034 settled that only the athlete's
own coach or the athlete may assign a programme, so nobody was ever going to
prescribe them one. ADR-0033 let a member put *themselves* on a published
programme, which helps — but a published programme is somebody else's plan, and
an Open Gym member walking in on a Tuesday to do their own thing is not looking
for one. And `workout_sessions.assignment_id` was `NOT NULL`, so with no
assignment there was no session, and with no session there was nowhere for a
set to go.

So the app tracked nothing for the membership the gym sells most of. Not
badly — *nothing*. No history, no estimated 1RM, no goal progress, no
attendance, nothing on Today except a card reading "No programme yet — ask at
the desk to be paired with a coach", which is advice to buy a different
membership.

This is the same shape as the three problems ADR-0033 named: **the product
could sell somebody a thing and then not let them use it.**

## Decision

**A session's plan link is optional. A session with none is an *unplanned*
session, and the member builds it themselves.**

Concretely:

1. `assignment_id` and `workout_template_id` become nullable, **both or
   neither** — enforced by a CHECK constraint, by the two domain constructors,
   and by a 400 from the application layer. There is no half-planned session.
2. A new `title` column holds what the member called it. Only an unplanned
   session may have one (a second CHECK), because a prescribed session is named
   by its workout template and a copy of that name here would go stale the
   moment a new version renames it.
3. `WorkoutSession::open(...)` is the constructor for one. It shares the
   `started_at` clock guard with the assigned path rather than reimplementing
   it.
4. Everything downstream is untouched.

Point 4 is the whole reason this is a small change rather than a subsystem.
`performed_sets.template_exercise_id` has been nullable since the first
execution migration, described there as *"work outside the plan"*. A session
that is **all** work outside the plan needs no new set machinery, no new
history table and no changes to progress: est-1RM, exercise history, goals and
attendance are all computed per `exercise_id` from `performed_sets`, and not
one of them reads the prescription. The founding rule holds unchanged —
prescribed and performed are two separate immutable records — an unplanned
session simply has no prescribed half.

### What the member sees

- **Today** offers *Start your own workout* to any member with no session
  already running. Not only to those without a programme: a coached member
  still does the odd extra session, and hiding it from them would say the app
  only counts training somebody else set.
- **`new-session`** asks for an optional name and lets them tick exercises from
  the gym's catalogue.
- **The logging screen** works as before, plus *Add an exercise* mid-session.

**Nothing on the picking screen is saved.** The chosen exercises travel to the
logging screen as a route parameter — a *starting list*, not a plan. There is
deliberately no table for "exercises I intend to do": it would be a second,
weaker copy of what `performed_sets` records the instant a set is logged, and
the two would disagree the first time somebody skipped something. A phone that
dies mid-session loses the list and rebuilds it from the sets already logged,
which is the copy that matters.

### The word

Called **unplanned**, not "open" — `SessionStatus::is_open()` already means
"still in progress" on the same type, and two senses of one word a method apart
is how somebody eventually writes the wrong one. The membership is still called
Open Gym; the session is unplanned.

## Consequences

**The joins that name a workout had to become LEFT joins.** `list` and
`find_session_view` inner-joined `workout_templates`, `program_assignments` and
`programs`. Left as they were, every unplanned session would have vanished from
every history list in the product — the member would log four sets and then not
be able to find the workout. `workout_name` and `program_name` are now
`Option<String>` up through `SessionView` and out of the API, which is honest:
there is no programme, so no name is invented for one.

**The freeze trigger had a latent bug that nullability would have activated.**
`reject_finished_session_edit` compared with `<>`, which is total while a column
is `NOT NULL` and stops being so the moment it is not: `NULL <> 'x'` is `NULL`,
an `IF` on `NULL` does not fire, and the guard would have quietly permitted an
unplanned session to be given an assignment after the fact. Every comparison is
now `IS DISTINCT FROM`. This was not caused by the change; it was uncovered by
it.

**Eight surfaces were spelling the session's display name themselves** and
already disagreed — "Workout", "your workout", "Programme". With a second
source for the name they would have drifted further, so `sessionName` /
`sessionNameFor` is now the single rule, with `verify-session-name.mjs` pinning
the case that matters: an unnamed own-workout and a workout whose name failed
to load must not read the same, because one is fine and the other is a bug.

**Unplanned sessions are excluded from programme progress.** `nextWorkout` is
fed only sessions with a `workout_template_id`, so training on your own between
coached days does not advance you through your weeks. That is the correct
reading — you did not do Day 3 — and it is the one line that would have been
silently wrong.

**What this does not do.** It does not let a member author a programme —
ADR-0034 moved authoring to head coach and above precisely to stop five people
fragmenting the catalogue, and an unplanned session writes no catalogue rows. It
grants no new read access: the same people who could see a member's sessions
before can see these. And it does not touch `may_coach_athlete` — this is a
member acting on themselves, which is the ADR-0033 pattern again.

**Verified by** `scripts/verify-unplanned-sessions.sh` (42 assertions),
`scripts/verify-session-name.mjs` (12), and eight domain tests.

## Alternatives considered

**Give the gym a hidden "Open Gym" programme and assign everybody to it.**
Rejected: it makes a prescription out of something nobody prescribed, and every
adherence number computed against it is fiction — a member who does five
exercises out of a template's eight has not been non-compliant, they have
trained. It also puts a programme in the library that no coach wrote and none
may edit.

**A separate `freeform_sessions` table.** Rejected: it duplicates the whole of
`workout_sessions` and `performed_sets`, and every downstream reader — history,
est-1RM, goals, attendance, the coach's session list — would need to union two
tables forever. Nullable columns on one table is the cheaper truth, and the
CHECK constraint makes the two shapes as distinguishable as two tables would.

**Require a name.** Rejected: the member came to train, not to fill in a form. A
blank name normalises to none and the UI calls it "Own workout".
