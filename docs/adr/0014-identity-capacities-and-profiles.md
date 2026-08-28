# ADR-0014: One account, many capacities — identity, roles, and profiles

- **Status:** Accepted (§2's "a trainer across many gyms" and §5's "personal gym" are
  superseded by [ADR-0023](0023-single-gym-deployment.md) — this deployment serves one
  gym. Everything else here — capacities as a set, person-owned profiles,
  invitation-based joining — is unchanged.)
- **Date:** 2026-07-18
- **Deciders:** Project author
- **Supersedes the membership shape in:** [ADR-0004](0004-postgres-shared-schema-multitenancy.md)
  (tenancy itself is unchanged; only the `memberships` structure)

## Context

The requirement, as stated: *"there's 1 type of user and a user can have 3 profiles — customer,
trainer, gym owner. A gym can have multiple owners and staff. A trainer can be affiliated with
multiple gyms."*

That is the right instinct, but "profile" is doing two jobs at once, and conflating them causes
the classic mess where permissions and personal data get tangled together. Separating them:

| Concept | What it answers | Scope |
|---------|-----------------|-------|
| **Account** | *Who are you?* | One per person, platform-wide |
| **Capacity** (role) | *What may you do **here**?* | Per gym |
| **Profile** | *What data describes you **as** a member / as a trainer?* | Per person, not per gym |

"Gym owner" is a **capacity**, not a profile — there is no personal data that describes you *as
an owner*. "Customer" and "trainer" are capacities that each happen to have profile data
attached. So it is not 3 profiles; it is **one account + N capacities + 2 profile types.**

The previous schema also had a real limitation: `memberships` allowed exactly **one role per
(user, gym)**. In reality a trainer at a gym is very often also a paying member there, and an
owner frequently coaches. That model cannot express it.

## Decision

### 1. One account, always

`users` stays global with no `gym_id`. A person **never** has a second login. Switching between
"my gym" and "the gym I train at" is a **context switch inside the app**, like Linear's
workspace switcher or Google's account switcher — not a different account.

### 2. Capacities are a set, not a single value

Replace "one role per (user, gym)" with **one row per granted capacity**:

```
gym_capacities(gym_id, user_id, capacity, granted_at, granted_by, revoked_at)
UNIQUE (gym_id, user_id, capacity) WHERE revoked_at IS NULL
```

Capacities: `owner`, `admin`, `head_coach`, `trainer`, `member`.

This directly delivers the requirements:
- **multiple owners per gym** — several users each hold `owner` in the same gym;
- **a trainer across many gyms** — the same user holds `trainer` in several gyms;
- **trainer who also trains there** — one user holds both `trainer` and `member` in one gym.

Effective permission is the **union** of held capacities. `owner` implies everything below it;
we compute that in one place rather than duplicating grants.

*(Renamed `instructor` → `trainer` to match the language owners and users actually use. The
glossary is updated; `head_coach` remains for the review workflow in ADR-0006.)*

### 3. Profiles are personal data, owned by the person

```
athlete_profiles(user_id PK, ...)   -- goals, training age, limitations
trainer_profiles(user_id PK, ...)   -- bio, certifications, specialties
```

Deliberately **no `gym_id`**: your goals and injury history are *yours* and follow you between
gyms — which is the whole point of one account. A gym sees a person's athlete profile *because*
that person holds `member` in that gym, not because the gym owns the row. Access is a
relationship question, answered by capacities (and enforced by RLS below).

This is consistent with ADR-0004: `gym_id` belongs on **tenant-owned** tables. A person's own
profile is user-owned, exactly like `users`.

### 4. Gyms are created by their owner, and joining is by invitation

- **Registering a gym** creates the gym and grants the creator `owner`.
- **Everyone else joins by invitation** — a gym invites an email with a set of capacities. If
  the email already has an account, accepting adds capacities to the existing account (never a
  second account). If not, they sign up and the invitation is consumed on first login.

This is the missing piece that makes "a user can belong to multiple gyms" reachable in the
product, not just representable in the schema.

### 5. Solo users get a personal gym, not a special case

An independent trainer (a real segment — see [market-analysis.md](../market-analysis.md)) needs
somewhere to keep clients and programmes without belonging to a commercial gym. Rather than
build a second, gym-less code path, they get a **personal gym** (`gyms.is_personal = true`) with
themselves as `owner`.

One tenancy model, one set of queries, one RLS policy set. The UI simply presents a personal
workspace instead of a gym. (GitHub does the same thing with personal accounts vs organisations.)

## Onboarding consequences

The model makes onboarding simple, which is the point:

1. **Create an account** — email, name, password. No role question yet.
2. **"What would you like to do first?"** — three plain choices, none of them permanent:
   - *Run a gym* → name it → you are its owner
   - *Coach clients* → join with an invite code, or start a personal workspace
   - *Train* → join with an invite code, or start solo
3. **Everything else is deferred.** Profile details, branches, staff — later, in context.

Capacities accrue over time; nothing asked at signup is irreversible. A user who later receives
a trainer invitation from a gym simply gains that capacity — no new account, no migration, no
"upgrade" flow.

## Alternatives considered

- **Three separate profile records as the primary model** (as literally described) — conflates
  permission with personal data. "Owner profile" would be an empty table, and it cannot express
  "trainer at gym A, member at gym B" without duplicating rows per gym anyway.
- **Separate accounts per capacity** — rejected outright; it is the thing users hate most about
  multi-role products, and it makes a shared training history impossible.
- **Single role column with a precedence order** (the previous design) — cannot express
  simultaneous trainer+member, which is common.
- **A distinct "solo" mode with no tenant** — doubles every query path and every policy for one
  user segment.

## Consequences

- **Positive:** every stated requirement is directly expressible; personal data follows the
  person between gyms; one tenancy model covers solo and commercial gyms; onboarding asks one
  reversible question.
- **Negative / costs:** permission checks now evaluate a *set* rather than a single value, so
  capability resolution must live in one place (`Capabilities`) and never be re-derived ad hoc.
  This is a breaking schema change — acceptable now, pre-launch, and much worse later.
- **Follow-ups:** invitations flow, the context switcher in the client, and profile editing.

## References

- [ADR-0004](0004-postgres-shared-schema-multitenancy.md), [ADR-0013](0013-mvp-application-layer-authorization.md),
  [domain-model.md](../domain-model.md), [market-analysis.md](../market-analysis.md)
