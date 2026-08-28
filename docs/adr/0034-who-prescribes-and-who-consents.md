# ADR-0034: The trainer prescribes, the gym curates, and a pairing takes consent

- **Status:** Accepted
- **Date:** 2026-08-26
- **Deciders:** Project author
- **Amends:** [ADR-0024](0024-trainer-authority.md) — reverses its authoring
  half, keeps its exercise-proposal half
- **Amends:** [ADR-0031](0031-standing-not-invitations.md) — restores a
  handshake on the *manager-initiated* pairing only; the member-initiated path
  it created is untouched

## Context

Three requests from the gym, which turn out to be one observation: **the owner
could do everything, so the owner did everything.**

1. Programme assignment was `can_manage_catalogue` OR the athlete's coach. The
   manager short-circuit made the owner the default prescriber for the whole
   gym — not because anyone decided that, but because the check let them.
2. A manager pairing a trainer with a member created an **active relationship
   immediately**. That relationship hands the trainer the member's entire
   training history, measurements and goals. The trainer was never asked.
   `crates/domain/src/coaching_request.rs` already argued, at length, that
   one-sided consent on a two-sided grant is wrong — and then applied that only
   to the member-initiated path.
3. Trainers could author programmes (ADR-0024). Five trainers can each write
   their own near-duplicate of "Beginner Strength", and progress is computed per
   `exercise_id` and per version (ADR-0018, ADR-0006), so the variants fragment
   the very longitudinal data the product exists to accumulate.

Underneath (1) and (2) is the same defect: **an authority check that is correct
for acting on other people, reused for a different question.** In (1),
`may_coach_athlete` answers "may I *see* this athlete's data", where a manager
genuinely must be able to say yes; reusing it for "may I *prescribe* for them"
conflated seeing with prescribing. In (2), `may_assign` correctly refused a
trainer pairing themselves — and then let a manager do it for them without
asking.

## Decision

### Prescribing is the trainer's, and only for their own clients

`AssignmentService::ensure_may_prescribe_for` — a new, narrow check with exactly
two answers:

- **the athlete themselves** (unchanged from the solo-training work), and
- **their own active coach**, by relationship, not by capacity.

A manager is not one of them. Deliberately *not* `may_coach_athlete`, which
short-circuits for managers and must keep doing so — that function answers the
visibility question, and the two are now separate in code as well as in
principle.

Withdrawal follows assignment, as it always has, so a coach or the athlete may
end it and a manager may not.

The consequence worth stating: an athlete with **no** coach can only be
prescribed for by themselves. That is correct — it is the solo-training path —
and it is why that path had to exist before this change could be made.

### The catalogue belongs to whoever runs the gym

`can_author_programs` moves from `can_coach` to `can_manage_catalogue`,
reversing ADR-0024's authoring half. A trainer **reads** the catalogue and
assigns from it.

ADR-0024's reasoning ("a draft binds nobody") was sound about *risk* and silent
about *duplication*, which is the cost that actually showed up. One curated
library is worth more than many private ones when every number the product
computes is keyed to a specific version.

`can_propose_exercises` **stays** at `can_coach`, and the reason is replaced
rather than kept: ADR-0024 argued a trainer who cannot name a movement cannot
write a programme, which is now moot. The better reason is that a trainer on the
floor is who notices the catalogue is missing something, and `proposed` status
means naming one commits the gym to nothing until a manager curates it.

Note that authoring and publishing are now the same set. They stay separate
questions because they are separate acts, and the **second-person approval rule
is now the only thing standing between one manager and the whole gym** — which
makes it more load-bearing than before, not less.

### A pairing the gym proposes takes the trainer's consent

`POST /coach-relationships` (direct creation) is **gone**. In its place,
`POST /coaching-requests/propose`: a manager names the pairing, it lands
**pending**, and the named trainer accepts. Accepting creates the relationship
in the same transaction, reusing the existing `answer` path.

`coaching_requests` gains `raised_by`. It is load-bearing, not informational:

- `is_proposal()` is `raised_by != athlete_id`.
- For a proposal, **only `coach_id` may answer.** Not the proposer, and not
  another manager — a handshake any manager can complete alone is decoration.
- For a member-raised request, a manager may still answer on the coach's
  behalf, so the pre-ADR-0031 rows are not stranded.

A manager may not propose **themselves** as the coach; that is the self-service
access grant `may_assign` refused, arriving by a new road.

ADR-0031 removed handshakes because they stalled the product — a coaching pair
waited on a coach opening an app they might not open until Thursday. That
argument still holds for the *member-initiated* path, which is why `choose`
still creates an accepted pairing outright: the member is granting access to
their own data, which is theirs to grant. It does **not** hold here, because the
person granting and the person being granted are both somebody other than the
member. The trainer's inbox is on Today, first, precisely because a proposal
nobody sees is how the old handshake failed.

## Consequences

- **Eight verify suites needed updating**, and every change was a fixture or a
  pinned old rule, not a weakened assertion: `verify-coaching` (52, was 41) and
  `verify-trainer-authority` (36) were substantially rewritten because they
  tested the removed behaviours directly; six others just needed pairing to go
  through the handshake, and two needed the *right actor* to assign.
- `RequestError::NotAManager` maps to **403**, not 400. Authorization refusals
  are not malformed requests, and the propose service checks the gate before any
  lookup so an unauthorised caller learns nothing about who exists.
- The mobile `authorPrograms` mirror flipped with the domain, which would have
  inverted the athlete screen's "Put them on a programme" row — showing it to
  the owner and hiding it from the trainer. That row is now gated on
  `mayPrescribeFor`, a pure relationship check mirroring the service.
- The manager's "Put an athlete on it" action is gone from the published-version
  screen. Its replacement as the next move is "Edit as a new draft", which is
  the only thing left to do with a frozen version.
- **Not built:** notifying a trainer that a proposal is waiting (the outbox from
  ADR-0027 exists and is where that belongs), and a manager view of proposals
  they have sent that nobody has answered. Both are visible gaps rather than
  hidden ones.
- **Deliberately unchanged:** ending a relationship is still manager-only
  (`may_assign`, which now gates only that). A trainer who wants out of a
  pairing has no self-service route, which is an asymmetry worth revisiting —
  they can decline a proposal but not resign from an accepted one.
