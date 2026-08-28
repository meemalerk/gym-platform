# ADR-0033: Group classes as weekly slots — and three things a member may now do alone

- **Status:** Accepted
- **Date:** 2026-08-26
- **Deciders:** Project author

## Context

Four reported problems turned out to share one shape: **the product could sell
somebody a thing and then not let them use it, or stop.**

1. **Card payment did nothing on a phone.** Checkout returned `201` with a
   `checkout_url` of `http://127.0.0.1:8080/pay/…`. On a device, 127.0.0.1 *is*
   the device. Same trap as `EXPO_PUBLIC_API_URL`, which the demo notes already
   warn about at length — and it was a configuration default, not a bug in any
   line of code.
2. **A member with no coach could train against nothing.** Assignment was
   coach-or-manager only. Somebody who joined through the open door (ADR-0031's
   one door) could read the entire programme library and put themselves on none
   of it, so the app tracked nothing for them: no session to open, no history,
   no progress. A catalogue of things they could not use.
3. **A member could join a plan but not leave one.** `subscribe` allowed
   `member_id == actor`; `cancel_subscription` was managers-only, and the mobile
   client had no function for it at all. The only way off a plan was to catch a
   manager.
4. **Classes did not exist**, though `Feature::ClassCredits` had been carrying
   scaffolding for them and the operating calendar (ADR-0015) had been built with
   "the calendar classes need" as an explicit motivation.

Underneath (2) and (3) is one pattern worth naming: **an authority check that is
correct for acting on other people, reused for acting on yourself.** Neither was
a security decision. One was a copied `ensure_may_coach`; the other a copied
`ensure_manager`.

## Decision

### A class is a weekly slot; a booking is a place in one dated sitting

Two tables. `gym_classes` holds "Zumba, Mondays at 18:00, cap 20" — a fact about
the timetable, changing rarely. `class_bookings` holds one member in one
occurrence, carrying `on_date`.

**Occurrences are derived, never stored.** The timetable read expands slots into
dates in SQL (`generate_series`, joined on `EXTRACT(DOW) = weekday`). The
obvious alternative — generating rows for future weeks — needs a job to mint
them, a decision about how far ahead, and a rewrite of every unheld occurrence
whenever the timetable is edited. Deriving has none of that.

Without `on_date` a booking would mean "Zumba, forever": somebody who came once
holds a place every Monday until a human notices.

**Weekday is 0 = Sunday**, matching `gym_opening_hours`, Postgres `EXTRACT(DOW)`
and JavaScript `getDay()`. Picking either convention is fine; picking neither and
converting in two places is how a Monday becomes a Sunday.

**Times are `TIME`, not `timestamptz`** — the same reasoning as ADR-0015. But the
*booking cutoff* ("has 18:00 Zumba started?") is decided on the **server**, so
this is where the gym's IANA zone must actually be resolved, and `chrono-tz`
becomes a dependency. Comparing in UTC would be wrong for every gym not on UTC,
silently and seasonally.

**Capacity is stated in the domain and made true by the database.** The domain
takes the live count as an argument rather than fetching it; a partial unique
index on `(class_id, on_date, member_id) WHERE cancelled_at IS NULL` is what
settles two members tapping Book in the same instant. Counting is racy by nature
and no amount of application code fixes that.

**Cancelling is a timestamp, never a delete.** That is what lets the same member
re-book (the index is partial), and what keeps "who was in Tuesday's HIIT"
answerable after the gym drops the class. Classes archive for the same reason.

**Publishing the timetable is gym management, not coaching.** A trainer teaches a
class; they do not decide the gym runs one on Mondays. The roster is readable by
the class's **own** instructor or a manager — not every trainer, because a roster
is a list of members by name and an uninvolved trainer has no reason to read it.

**Booking gates on `Feature::GymAccess`, not `ClassCredits`.** `ClassCredits`
still describes a per-pack balance that does not exist; gating on it would refuse
a place to every member of every gym, including the ones whose plans grant
`gym_access` and nothing else. `GymAccess` is the honest rule for "may this
person train here today" and is the rung the door already uses. When balances
arrive, `ClassService::book` is the one call site that changes.

### Review counts prescribed exercises, not weeks

`submit_for_review` took a **week count**. A week is a container, so a version
with one empty week — or with a workout prescribing nothing — passed review, was
approved, and was **published**, which freezes content for good (ADR-0006). It
was then assignable, and the athlete opened a workout on a blank screen that
nobody could ever repair.

Two programmes in the demo data were in exactly that state, which is how it was
found.

The gate now counts prescriptions. One prescription implies the workout and week
holding it, so a count of zero catches every empty shape at once.

### A member may put themselves on a published programme

Narrow and deliberate: `athlete == actor` only. This is self-service, not a
coaching right. Every other guard is untouched — the version must be published,
the athlete must be a current member, the tenancy scope is unchanged.

`may_coach_athlete`'s "self grants nothing" rule is **not** changed. That rule is
about authority over an athlete's data and coaching, and it still holds; the
bypass lives in `AssignmentService::ensure_may_coach`, for programme assignment
alone.

Symmetrically, a member may **withdraw** themselves — including from a programme
a coach set. Choosing a plan and stopping following it are the same right, and
the withdrawal is audited, so the coach sees it happened rather than finding a
silently abandoned programme.

### A member may end their own subscription

Any time, without asking. The money does not change: access runs to the end of
the period already paid for, computed exactly as before, and no refund is
invented.

**Cancelling is not leaving the gym.** Membership standing is untouched, so
"coached → solo" is this cancel followed by subscribing to a plan granting
`gym_access` alone — both of which a member can now do themselves.

A manager can still cancel on somebody's behalf: the right was widened, not
moved. Somebody else's subscription reads as **404, not 403** — whether a given
id exists is not a thing to confirm to a stranger.

## Consequences

- `chrono-tz` is a new dependency, and load-bearing. Removing it means either
  storing instants (wrong across DST) or comparing in UTC (wrong for most gyms).
- Two verify suites had pinned the old rules — "member self-assigns → 403",
  "member withdraws themselves → 403". Both were updated to the new rules with
  the reason recorded inline, and the assertion that *still* matters (a member
  may not touch anybody else) is now stated explicitly rather than implied.
- Five verify fixtures built programmes with weeks but no exercises and so broke
  under the stricter review gate. They were given real content rather than the
  rule being weakened — the fixtures were building unusable programmes.
- `ApplicationError` gained `StateConflict(String)`. The existing `Conflict`
  renders through an `"{0} already exists"` template, which turned "Zumba is
  full (20 places)" into nonsense. Duplicates and state conflicts are two kinds
  and are now two variants; both are 409.
- **Not built:** class credits and packs, waitlists for a full class, recurring
  cancellation ("skip next Monday"), and attendance marking against a booking.
  The last is the natural next step — a booking is currently a place, not a
  record of turning up.
- **Known inconsistency, deliberately left:** cancelling an already-cancelled
  subscription returns 400 (the domain reports it as `DomainError::Invalid`)
  while the route's OpenAPI block advertises 409. The mapping is shared with the
  billing worker, so moving it is a change to make deliberately rather than in
  passing. `verify-billing.sh` pins the behaviour as it actually is.
