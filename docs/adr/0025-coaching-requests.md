# ADR-0025: A coaching relationship takes consent from both sides

- **Status:** Superseded by [ADR-0031](0031-standing-not-invitations.md) — the
  handshake is gone; a member chooses a coach and is coached from that moment.
  What survives is the trainer *directory*, the anti-probing 404, and the rule
  that a coach cannot assign themselves a client.
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Extends:** [ADR-0005](0005-relationship-based-authorization.md). The relationship model
  is unchanged; this adds the missing way to create one.

## Context

`coach_relationships` could only ever be created by a head coach or above
(`CoachRelationship::may_assign`). That restriction is correct and deliberate: the
relationship is an **access grant** — `may_coach_athlete` opens an athlete's whole training
history, measurements and goals to another person — so letting a trainer create one for
themselves would turn coaching into a self-service permission grant over any member's data.

But it left the ordinary case with no route at all. A member who joins and wants a coach has
to find a head coach, in person, and ask them to make a database entry. The product had no
answer for the single most common thing a gym member wants.

The two obvious shortcuts are both wrong, in opposite directions:

- **Let the member pick a coach and pair immediately.** One tap and a member has handed
  someone their full training history, and a trainer has acquired a client without being
  asked. One-sided consent on a two-sided grant.
- **Let the trainer pick their clients.** This is exactly what `may_assign` refuses, for
  exactly the reason above.

There is also a privacy constraint that any solution has to survive.
`GymService::roster` is head-coach-only on purpose — "who trains at this gym" is not
something a member agreed to share with every other member. A "browse and choose" feature
must not become a roster leak by the back door.

## Decision

**The member asks; the coach answers.** A `CoachingRequest` with the lifecycle
`Pending → Accepted | Declined | Withdrawn`, resolved and never deleted, mirroring how
`coach_relationships` already behaves.

Four things make it work:

1. **Accepting creates the relationship in the same transaction.** Not two calls. A window
   in which a member has been told yes and their coach still cannot see them would be
   invisible in testing — both calls succeeded — and infuriating in use. Same reasoning as
   writing the audit entry inside the transaction it describes.

2. **A trainer *directory*, which is not the roster.** `GET /gyms/{id}/trainers` returns
   only what each coach published about themselves professionally: name, headline, bio,
   specialties, certifications, and how many people they currently coach. No emails, no
   member names. That is why it can be open to every member while the roster stays closed,
   and `verify-coaching-requests.sh` asserts both halves — the directory being readable
   *and* the roster still 403-ing for the same caller.

3. **A manager may answer on a coach's behalf.** Someone has to be able to clear a queue
   when a trainer leaves, and a manager could have created the pairing outright anyway, so
   this grants them nothing they did not have.

4. **Only the asker withdraws.** A coach who does not want a client *declines* — a
   different and more honest record. `Withdrawn` is kept distinct from `Declined` because a
   gym looking at a pile of unanswered requests needs to know whether it is losing people
   to slow replies or they sorted themselves out.

### Refusals do not confirm who exists

Asking about someone who is not a coach here returns **404**, and so does asking about
someone who is not in the gym at all. Distinguishing them would make the endpoint a
membership oracle: post a user id, read the status code, learn whether that person trains
here. The invitation flow already refuses to be probed this way and this matches it. The
directory is the sanctioned way to discover who coaches here, and it lists exactly the
people for whom the call succeeds.

Asking *yourself* is allowed to be a plain 400 — you already know you exist, so there is
nothing to leak, and a useful message beats an evasive one. In practice that path is only
reachable by someone who genuinely coaches; a member pointing at their own id is caught one
step earlier by "not a coach here".

### One pending request per pair

Enforced by a partial unique index on `status = 'pending'`, so a member cannot spam one
coach — but asking again after a decline is fine. Circumstances change, and a permanent
block would be a strange thing for a gym to enforce on its own members.

## Consequences

**Good.** The gym gains the flow it was missing, without weakening a single existing rule:
the roster stays private, `may_assign` still refuses self-service pairing, and the grant is
now made by the two people it concerns. Unmet demand becomes visible as a queue instead of
silence, which is operationally useful in its own right — a gym can see that four members
are waiting on a coach who has not opened the app in a week.

**Cost.** A new table, a new lifecycle, and a queue that someone has to work. A gym that
ignores its requests leaves members waiting, which is worse than the previous state where
they at least knew nothing was happening. Surfacing pending requests at the *top* of the
People tab, above every list, is the mitigation — and it is the only content on that screen
that is waiting on the person reading it.

**Not built.** No notification when a request arrives; that needs the outbox worker, and
faking it with a polled badge would be a second delivery mechanism to retire later. Until
then the queue is visible on open, which is honest about what it is.

**Verified by** `scripts/verify-coaching-requests.sh` (46 HTTP assertions), the domain unit
tests in `crates/domain/src/coaching_request.rs`, and a direct privilege assertion that the
app role holds `INSERT, SELECT, UPDATE` and not `DELETE` — a check that caught this
migration granting DELETE by inheritance, exactly as migration 0012 warns.
