# ADR-0031: One door in, standing set from the roster — and three other handshakes removed

- **Status:** Accepted
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Supersedes:** [ADR-0025](0025-coaching-requests.md) entirely; the invitation
  half of [ADR-0014](0014-identity-capacities-and-profiles.md) and
  [ADR-0026](0026-open-registration.md)'s "both doors"

## Context

Four separate features shipped, each individually defensible, and together they
made the product impossible to get through end to end. A person setting up a
gym and trying to reach "a member does the workout their coach wrote" hit a wall
at every stage:

1. **Invitations could not be completed.** Staff existed only by invitation: a
   manager posted an address, the server minted a single-use token bound to it,
   and redeeming it granted capacities. Careful work — hashed tokens, an
   anti-probing 404, a conditional `UPDATE` so two concurrent redemptions could
   not both grant. All of it in service of a flow whose second step is *read an
   email*, and there is no mail adapter behind the `EmailSender` port. The token
   was returned in the API response, so the only way to use the feature was to
   copy it out of a response body by hand.

2. **A coaching relationship needed the coach to press a button.** A member
   chose a coach; a request went pending; the coach accepted it in the app.
   Since nothing downstream can happen until the pairing exists — no programme,
   no assignment, no workout — the whole product waited on somebody opening
   their phone.

3. **A programme could reach a state with no legal move out of it.** Publishing
   requires approval by somebody other than the author, unless the gym is
   flagged `is_personal` at creation. An ordinary gym is not, and an ordinary
   new gym has exactly one person who can publish. So the owner wrote a
   programme, submitted it for review, and then could not approve it. Nobody
   could. The lifecycle stopped dead at "in review".

4. **A member could not start a workout, and could not fix it.** Once a gym has
   any plan on sale, `ExecutionService::start` requires an active subscription
   granting `gym_access`. Subscribing was managers-only. So the refusal named
   the three plans that would have let them in and offered no way to be on one
   — the app knew exactly what was wrong and did nothing about it.

Two smaller faults belong to the same family, because they are the same mistake
— a screen that knows what is wrong and does not act on it:

5. **Manage → Exercise library went to the dashboard.** It pushed
   `/(app)/(tabs)/library`, and the Library *tab* is `Tabs.Protected`-unmounted
   for anyone who can manage the gym (they trade it for Billing under the
   five-tab ceiling). The route matched nothing, so expo-router fell through to
   its built-in **development** unmatched screen — which is where the stray
   `(tabs)` button came from, and why pressing it did nothing.

6. **The programme screen was six chips in a tray.** Add week, Submit, Approve,
   Back to draft, Publish, Assign, Edit as new draft — all the same size, in the
   order they were written. Nothing said which stage the version was at, which
   chip came next, or why Approve sometimes failed.

## Decision

### One door, and standing is set afterwards

Invitations are **removed**: domain, service, repository, routes, table, RLS
policies and suite. Everybody enters through the open door (ADR-0026) as a
`member`, and a manager sets what they hold from the roster —
`PUT /gyms/{id}/members/{user}/capacities`.

The rules that mattered survive, in `check_standing_change`, with tests:

- owners and admins may change standing; **only an owner may grant or remove
  `owner`**, or `admin` is `owner` with extra steps;
- the **last owner cannot step down** — a gym with no owner cannot appoint one;
- standing is **replaced, not amended**, so promotion and demotion are one call
  and cannot disagree;
- standing cannot be emptied. Removing somebody from the gym touches their
  history and their coaching links, and is deliberately not reachable from here.

This makes the closed-gym case harsher and it is stated rather than hidden: a
gym with its door shut admits nobody, including people the owner wants as staff.
That is one switch, in Settings, next to the sentence explaining it.

### Choosing a coach is one step

`CoachingRequest::choose` returns a request that is **already accepted**, and
the pairing is written in the same transaction. The thing a member grants is
access to their **own** training history, which is theirs to grant.

The coach is not trapped: ending a relationship is still one call and still
theirs. Note the asymmetry is deliberate — a *coach* still cannot assign
themselves a client, because that would be a self-service grant of access to
somebody else's data. Ending is the opposite of that.

`answer` and `withdraw` stay on the server for rows raised before this change.
Neither client offers them; nothing can create a pending request any more.

### The second-person rule stands down when there is no second person

`approval_policy` now counts **other people who could publish**, rather than
reading a flag chosen at gym creation. No other reviewer → self-approval is
allowed. Promote a head coach and the rule returns, with nobody changing a
setting.

A second-person rule protects a gym from one person pushing unreviewed work to
everybody. Where there is no second person it protects against nothing and
prevents everything.

### A member may put themselves on a plan

`BillingService::subscribe` accepts two callers: a manager subscribing anybody,
or a member subscribing **themselves**, to a plan that is **currently offered**,
when they are **not already on one**. Price and grants are still the gym's,
copied at signup exactly as before. The only choice the member makes is which of
the gym's own offers to take.

The refusal on the programme screen now carries the button that fixes it.

### The two UI faults

`LibraryScreen` moved to `src/ui/` and is rendered by both the tab and a new
pushed route `/(app)/library`, which Manage links to. A `+not-found.tsx` now
exists, so a bad route can never again show expo-router's development screen to
a member.

The programme screen became **a place plus one next move**: a four-stage strip
(Draft → In review → Approved → Published), one primary button for the move that
actually comes next, and a sentence saying what it will do. Refusals that used
to arrive as a red banner after pressing — no weeks yet, somebody else has to
approve — are printed before the press. Everything else demotes to a quiet row.

## Alternatives considered

- **Fix invitations instead: add an SMTP adapter.** A real option, and a bigger
  one — a mail provider, deliverability, a bounce path, and the invitation flow
  still only does what promoting from the roster does in two taps without
  leaving the app.
- **Keep the coaching handshake and fix the accept screen.** The screen was not
  broken; the wait was. Any amount of polish on a queue still leaves the member
  unable to train until somebody else acts.
- **Drop the workout gate entirely.** Tempting — blocking the *log* means the
  member trains anyway and does not record it, and the training history is this
  product's whole advantage. Rejected because a gym that sells memberships has a
  real reason to gate, and self-service subscription gives the member the same
  outcome without giving up the model. The gate stays where it is
  (`ExecutionService::start`, not the route), so a future offline replay goes
  through it too.
- **Let anyone with `can_manage_gym` grant `owner`.** One fewer rule, and it
  makes `admin` and `owner` the same capacity in two spellings.

## Consequences

- **Positive:** the demo path works start to finish — sign up, join, be
  promoted to trainer, write and publish a programme, assign it, and see it on
  the member's Today with a plan they put themselves on. Three tables' worth of
  token-handling machinery is gone. Standing changes are audited in one place.
- **Negative / costs:** a closed gym now admits nobody at all, which is a real
  loss of an escape hatch — mitigated only by the fact that the escape hatch
  could not be used. Two-sided consent for coaching is gone, which is a genuine
  reduction in what a coach controls; ending is the compensating control.
  Self-service subscription is one more path that writes money records, and it
  is deliberately the narrowest one that closes the dead end.
- **Migration:** `20260824000030_remove_invitations.sql` drops the table and its
  policies. Accepted invitations already left their mark in `gym_capacities`
  and the audit log; a pending one was only ever a link somebody could click.

## Verification

- `scripts/verify-capacities.sh` (**29 checks**) replaces
  `verify-invitations.sh`: who may change standing, the owner rule, the
  last-owner rule, replace-not-amend, unknown capacities as 400, and that all
  three invitation endpoints now 404.
- `scripts/verify-coaching-requests.sh` (**37**) rewritten: choosing pairs
  immediately, the directory is still not the roster, the endpoint is still not
  a membership oracle, and a resolved record cannot be answered.
- `scripts/verify-programs.sh` (**59**, was 55) gains the case that was
  unreachable before: the only person who can publish may approve their own
  work, and the moment somebody else can review, self-approval is refused
  again.
- **Nineteen suites** provisioned their users through invitations. Their
  helpers were re-pointed at join-then-promote, keeping the call shape so the
  suites still read the same; all of them pass.
- `verify-open-registration.sh` §5 changed from "invitations still work
  alongside" to "the door is the only way in".

## References

- [ADR-0025](0025-coaching-requests.md) — the handshake this removes
- [ADR-0026](0026-open-registration.md) — the door that is now the only one
- [ADR-0014](0014-identity-capacities-and-profiles.md) — capacities as a set
- [ADR-0010](0010-payments-and-billing.md) — the billing model self-service sits inside
- [ADR-0029](0029-auth-hardening.md) — the `EmailSender` port with no adapter
