# ADR-0015: One operating calendar — pattern plus dated overrides

- **Status:** Accepted
- **Date:** 2026-07-18
- **Deciders:** Project author

## Context

Three things were requested separately — **weekly schedules**, **gym closure days**, and
**special opening hours** — and they are the same question asked three ways: *is this gym open at
this moment, and if so, until when?*

Everything downstream depends on that one answer. A booking cannot be offered on a closed day. A
reminder must not fire for a session on a public holiday. A trainer's availability is meaningless
outside the gym's opening hours. Modelled separately, the answer gets computed three times and
the three will disagree — which surfaces to a member as an app that offers a slot the gym then
refuses to honour.

Two things make this harder than it looks:

1. **Timezones.** A gym is a physical place in one timezone. Storing "opens at 06:00" as a
   `timestamptz` is wrong — 06:00 local stays 06:00 across a DST boundary, whereas an absolute
   instant does not. Get this wrong and every gym in a DST-observing country is an hour wrong
   twice a year, in a way that looks like a caching bug.
2. **Recurrence is a known swamp.** Full RRULE support (RFC 5545) is where calendar features go to
   die: infinite expansion, exceptions to exceptions, and a query planner that cannot use an index.

## Decision

Model the calendar as **a recurring weekly pattern plus dated overrides**, resolved by a single
function that everything else calls.

```
gyms.timezone                  IANA name, e.g. "Asia/Karachi". Not an offset — offsets change.

gym_opening_hours              the recurring pattern
  gym_id, weekday (0-6), opens_at TIME, closes_at TIME
  (multiple rows per weekday allow split hours: 06:00-10:00, 16:00-22:00)

gym_calendar_overrides         a specific date that differs from the pattern
  gym_id, date, is_closed, opens_at TIME NULL, closes_at TIME NULL, reason
```

**Resolution rule, in one place:** for a given date, if an override row exists it wins entirely —
either closed, or those hours. Otherwise the weekday pattern applies. There is no merging, no
partial override, no precedence ladder. One rule, stated once, implemented once.

- **Times are stored as `TIME`, not `timestamptz`,** and interpreted in the gym's IANA timezone at
  the moment of resolution. "Opens at 06:00" is a wall-clock fact about a place, and it survives
  DST because it was never an instant to begin with.
- **`is_closed` is explicit** rather than inferred from null hours. "Closed on Christmas Day" and
  "hours not configured" are different states, and a system that confuses them either turns
  people away or lets them in.
- **No RRULE.** Recurrence is exactly "weekly by weekday". Annual holidays are generated as
  override rows for a horizon (a year or two ahead) rather than expanded from a rule at query
  time. Rows are inspectable, indexable and editable one at a time — a holiday that moves is one
  UPDATE, not a rule nobody dares touch.
- **`trainer_availability` reuses the identical shape.** A trainer's bookable time is the
  intersection of their availability and the gym's opening hours, and the intersection is only
  cheap because both sides are the same structure.
- **Branch-ready, not branch-built.** [ADR-0004](0004-postgres-shared-schema-multitenancy.md)
  anticipates `branches`, which do not exist yet. These tables key on `gym_id` now; the column
  becomes `branch_id` when branches land. Building branch support before branches exist would be
  Phase N+1 infrastructure inside Phase N, which the roadmap forbids.

**A "weekly schedule" is therefore a view, not a table.** What is on this week is resolved from
opening hours, overrides, trainer availability and (later) scheduled workouts and classes. There
is no separate schedule entity to fall out of sync.

## Alternatives considered

- **Full RRULE / iCalendar recurrence.** Genuinely expressive, and correct for a calendar
  application. Rejected: this is a gym, not Google Calendar. The cost is unbounded complexity in
  expansion and querying, for expressiveness ("every second Tuesday except in August") no gym has
  asked for. Pattern-plus-override covers every real case at a fraction of the cost.
- **Materialise every day into a row** (one row per gym per date, generated ahead). Simplest
  possible query — no resolution logic at all — but the table grows without bound, a change to
  normal hours means rewriting the future, and it answers nothing about dates past the horizon.
  Considered seriously; rejected on the write amplification.
- **Three separate features** (schedule, closures, special hours), as requested. Rejected: they
  would each need their own resolution against the others, and the first inconsistency between
  them is a member turning up to a locked door.
- **Store opening hours as `timestamptz`.** Rejected outright — wrong across DST, and the bug is
  silent and seasonal, which is the worst combination to debug.

## Consequences

- **Positive:** one source of truth for "open or not"; DST-correct by construction; overrides are
  ordinary rows a gym admin can see and edit; trainer availability comes almost free; the weekly
  schedule cannot disagree with the closure list because it is derived from it.
- **Negative / costs:** resolution is a function call rather than a column read, so anything
  hot-path needs care (cache per gym per day if it ever matters — not before). Annual holidays
  need generating on a horizon rather than living as a rule. Split hours mean multiple rows per
  weekday, so callers must handle a list rather than one interval.
- **Follow-ups:** classes and bookings ([roadmap.md](../roadmap.md) Phase 6+) must resolve against
  this and never against their own copy of the hours. When `branches` land, migrate the key.

## References

- [feature-plan-2026-07.md](../feature-plan-2026-07.md) §7
- [ADR-0004](0004-postgres-shared-schema-multitenancy.md) — tenancy and the branch anticipation
