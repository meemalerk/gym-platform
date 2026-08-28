# ADR-0026: Both doors — open registration alongside invitations

- **Status:** Accepted, amended by [ADR-0031](0031-standing-not-invitations.md)
  — invitations were removed, so "both doors" is now one. The open door still
  grants `member` and nothing else; staff are promoted from the roster.
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Refines:** [ADR-0023](0023-single-gym-deployment.md). Single-gym deployment is unchanged;
  this adds the way in that ADR-0023 removed without replacing.

## Context

Sign-up was a dead end.

Creating an account worked. After that, an account with no membership was routed to
onboarding, and onboarding had exactly one screen: *paste your invite code*. ADR-0023 had
removed the previous escape hatch — "everyone gets a personal gym" — and nothing took its
place, so a person who downloaded the app and walked into the gym could not get in unless a
member of staff generated a token and read it out to them.

The sign-in screen, meanwhile, was still promising *"a personal gym is set up for you
automatically"* — copy for a feature that had been deleted. The product was advertising a
door that no longer existed and hiding the fact that the remaining one needed a key.

## Decision

**Two doors, and the owner holds the switch for the second one.**

`gyms.open_registration`, a boolean, default **false**. When it is on, any authenticated
account may join that gym as a plain member. Invitations are untouched and remain the only
way to gain staff standing.

Three properties make this safe:

1. **The open door admits members and nothing else.** The capacity is a string literal in
   the repository's INSERT — not a parameter, not read from the request body, not derived
   from anything the caller controls. There is no input that turns joining into a way to
   make yourself an owner. `verify-open-registration.sh` posts forged `capacities` arrays at
   the endpoint and asserts the result is still exactly `['member']`, because this is the
   single worst bug this feature could grow.

2. **The door is re-checked inside the transaction**, with `SELECT ... FOR UPDATE`. An owner
   closing registration and a stranger walking through it is a real race, and the door has
   to win it.

3. **A closed gym is not discoverable.** "No such gym" and "that gym is not accepting
   members" both return 404. Otherwise the endpoint becomes a directory of every gym on the
   deployment, probeable by id.

Opening and closing are `can_manage_gym` — owners and admins, deliberately narrower than
`can_manage_catalogue`. Who may walk into the gym is a settings decision about the business,
not a coaching one, and a head coach should not be able to change the gym's membership
policy. Both the read and the write sit at that level, so there is one rule rather than two.

### The database had to learn two new things

Both were found by the verification suite rather than by reading the code, and both are the
same shape of mistake: **under `FORCE ROW LEVEL SECURITY`, "no policy" means "no rows", not
"no restriction".**

- `gyms` had never been UPDATEd — the row was written once at creation and only read
  afterwards — so the table carried SELECT and INSERT policies and no UPDATE policy at all.
  The settings UPDATE silently matched zero rows and the service reported the gym as
  missing. Fixed by `gyms_update`, scoped to the active tenant exactly like `gyms_read`.

- The discovery read runs with **no tenant context**, because the caller has no membership —
  that is the state it exists to resolve. Neither existing SELECT policy matched, so the
  open list was permanently empty. Fixed by `gyms_read_open`, whose predicate *is* the
  access rule: a gym is readable without context precisely because its owner published it as
  joinable.

### Solo is not a mode

After joining, the app asks *"how do you want to train?"* — with a trainer, or on your own.

That screen writes **nothing**. There is no `is_solo` column and there must never be one. A
member training alone is simply a member with no coach relationship, which
`coach_relationships` already expresses; a flag would be a second source of truth for a
question the model answers, and its first bug would be someone marked "solo" who has a
coach. The screen is a signpost: one branch opens the coach directory, the other opens
Today. That is the whole mechanism, and it is what lets the copy honestly say the choice can
be changed at any time.

It is not a gate either. Force-quitting on it leaves an ordinary member who lands on Today
next launch — the same place the "on my own" branch leads.

## Consequences

**Good.** The product can onboard a walk-in. The gym decides whether it wants to; the
default is closed, so a deployment that has not thought about it is not quietly accepting
strangers. Staff onboarding is unchanged and still requires a deliberate invitation naming
the standing granted.

**Cost.** One more thing an owner can misconfigure, and a public-ish surface where there was
none. The mitigations are that the surface is tiny (id, name, slug of gyms that opted in),
the grant is fixed at `member`, and every open/close and every self-join is audited — so
"when did we start letting people in?" has an answer.

**Not built.** No email verification, so an open gym accepts unverified addresses. That
belongs with the auth-hardening work and is called out there rather than pretended away
here; a gym that cares should leave the door closed until it lands.

**Verified by** `scripts/verify-open-registration.sh` (28 HTTP assertions), which covers the
default-closed state, who may flip the switch, the escalation attempts, that closing does
not evict, that invitations still work into a closed gym, and that every change is audited.
