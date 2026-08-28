# ADR-0023: Single-gym deployment

- **Status:** Accepted
- **Date:** 2026-08-23
- **Deciders:** Project author
- **Supersedes:** the multi-gym-per-account part of [ADR-0014](0014-identity-capacities-and-profiles.md)
  §2 ("a trainer across many gyms") and §5 ("solo users get a personal gym"). ADR-0014's
  other decisions — capacities as a set, profiles as person-owned data, invitation-based
  joining — are unchanged.

## Context

The product was built as a platform of many gyms: any account could create a gym or
accept an invitation to a second, third, or Nth one, and `gym_capacities` let a single
`user_id` hold capacities across all of them simultaneously — demonstrated end to end by
`multi@demo.test` belonging to three gyms with different standing in each.

The decision is to build a **single-gym product** instead: one deployment, one gym. No
gym marketplace, no "create another gym," no "join a second gym," no gym-switcher. An
account either belongs to *the* gym or it doesn't.

## Decision

**Keep the multi-tenant engine; remove the tenancy choice.** `gym_id` stays on all 19
tenant-owned tables, RLS stays keyed on it, `TenantScope` stays exactly as it is, every
`/gyms/{gym_id}/...` route is untouched. Rewriting any of that — dropping columns,
rewriting RLS, collapsing `TenantScope` into a gym-less extractor — would touch ~41 routes
across 13 route files for zero product benefit: RLS-by-`gym_id` with cardinality 1 is
still valid, harmless defense-in-depth, the same "enforce twice, app + DB" pattern already
used for program-version immutability ([ADR-0006](0006-immutable-program-versioning.md)).

What actually changes:

1. **`gyms` is capped to one row, enforced in the database — but as a per-deployment
   policy, not a hard schema limit.** A `BEFORE INSERT` trigger (migration
   `20260823000019`) rejects a second gym only when `app.single_gym_mode` is `'true'` for
   the transaction; `PgGymRepository` sets that GUC from `Config::single_gym_mode` (env
   `SINGLE_GYM_MODE`) before every gym creation. **Off by default**: the verification
   suites (`scripts/verify-rls.sh`) deliberately create
   several gyms through the same public API to prove tenant isolation, and a blanket cap
   would break that methodology, not just the demo. A real single-gym deployment turns it
   on explicitly — see `docker-compose.demo.yml` — the same shape as `APP_ENV` gating the
   JWT-secret placeholder check. The EXISTS check still needs `SECURITY DEFINER`
   regardless of the flag: `gyms_read` only shows the *active* gym, so `gym_app` cannot
   see whether a different gym already exists without running as the owning (superuser)
   role.
2. **Bootstrap stays a one-time, non-UI action.** The first `POST /api/v1/gyms` call still
   creates the gym + its owner (today's existing flow, unchanged); every call after that
   gets 409. Regular sign-up still requires an invitation to gain a capacity — no
   auto-enrollment was added, since that's a separate, bigger security-policy question.
3. **`/api/v1/me` keeps returning `memberships` as a list — deliberately not narrowed to
   an optional single value.** The backend stays multi-gym-observable for the same reason
   it stays multi-gym-*capable*: `SINGLE_GYM_MODE` caps gym creation, not how many
   pre-existing gyms an account can hold capacities in, and `scripts/verify-rls.sh`
   depends on `/me` reporting more than one to prove that. The **mobile client** is what
   commits to singular — `toMembership()` in `apps/mobile/src/api/gym.ts` takes the first
   entry — which is honest for what it actually is: in a real single-gym deployment the
   list is never longer than one anyway.
4. **The mobile client drops everything built for choosing among gyms**: the gym-switcher
   screen, the "Your gyms" list, the onboarding choice between creating a gym and joining
   one (only "join," i.e. redeem an invitation, remains — creating the one gym is now an
   ops action, not an onboarding path).
5. **`is_personal` on `Gym` is left in place but is vestigial.** "Personal gym" was
   specifically the multi-gym pattern for solo users; with cardinality 1 there is no
   second, commercial gym for a personal one to be distinct from. Dropping the column is a
   separate, smaller follow-up, not bundled into this change.

## Alternatives considered

- **Full tenancy rewrite** (drop `gym_id`, RLS, `TenantScope`; hardcode single-tenant) —
  rejected. Touches 19 tables and ~41 routes, invalidates `scripts/verify-rls.sh`'s
  isolation proof, and buys nothing a one-row cap doesn't already buy. The tenancy engine
  is cheap to keep and remains genuine defense-in-depth against a future bug that forgets
  a `WHERE gym_id = ...` clause.
- **Cap membership per user instead of per deployment** (many gyms may still exist, but
  one account may only ever belong to one) — considered, but explicitly not what was
  asked for: the requirement is that the *deployment* serves one gym, not that the
  platform hosts many gyms which happen to be siloed per account.
- **Auto-enroll every sign-up as a member** — rejected for now as an undiscussed policy
  change (open self-registration into a real gym, versus today's invite-gated model).
  Flagged as a live option if wanted later.

## Consequences

- **Positive:** the onboarding, navigation, and session-state surface all get simpler —
  `memberships: Membership[]` + `activeGymId` collapses to one nullable `membership`, and
  an entire screen (gym-switcher) plus an onboarding branch (create-a-gym) disappear.
  Tenant-isolation machinery keeps being exercised and proven by `verify-rls.sh`, unaffected.
- **Negative / costs:** `is_personal` and the domain's "personal workspace" framing become
  dead weight until a follow-up removes them. The business-model docs
  (`docs/market-analysis.md`, `docs/cost-analysis.md`, `docs/subscriptions-billing.md`)
  describe a B2B2C SaaS sold to *many* gym-org customers — that thesis is now inconsistent
  with "each deployment is one gym" and is flagged in those docs rather than rewritten
  here; reconciling the business model is a separate decision.
- **Follow-ups:** drop `is_personal` and `Gym::new_with_kind`'s personal/commercial
  distinction; decide whether sign-up should auto-enroll into the one gym; reconcile the
  market/cost/billing docs with a single-gym-per-deployment model (or decide this ADR
  implies "one deployment per gym customer," which is a different framing of the same
  business model, not a contradiction of it — worth writing up explicitly if that's the
  intent).

## References

- [ADR-0004](0004-postgres-shared-schema-multitenancy.md) (the tenancy engine, kept),
  [ADR-0014](0014-identity-capacities-and-profiles.md) (capacities-as-a-set, kept; the
  many-gyms framing, superseded here), [ADR-0006](0006-immutable-program-versioning.md)
  (the "enforce twice" pattern this reuses), `docs/test-accounts.md`,
  `scripts/seed-demo.sh`.
