# ADR-0022: The demo seed exists to demonstrate the rules, not to fill tables

- **Status:** Accepted
- **Date:** 2026-08-01
- **Deciders:** Areeb

## Context

`scripts/seed-demo.sh` started as convenience: a few accounts so you did not have to sign up by
hand. It grew to three gyms, published programmes, coaching pairs, weeks of session history,
measurements, goals and billing.

It was still, in an important sense, useless — because everything it created described **a gym
going well**. Every programme was published. Every goal was open. Every coaching pair was
active. Every invoice was due or paid.

But this product's substance is almost entirely in what happens when things *do not* go well:

- a version cannot be edited once published, so editing forks a draft;
- a coaching pair ends without being deleted, because the sessions that coach saw were
  legitimately seen;
- an invoice is corrected by voiding it and issuing another, never by editing it;
- a goal closes as achieved or abandoned, and neither erases anything.

None of that is visible in a happy-path fixture. A demo that shows only the happy path
demonstrates a to-do list with a gym theme, and reviewers reasonably conclude that is what it is.

## Decision

We will treat the seed as a **demonstration of the invariants**, and hold it to that standard:
every rule the product is built around must be visible in seeded data without anyone having to
perform a workflow first.

Specifically the seed must always produce, alongside the happy path:

- a programme left in **draft**, so the authoring UI has something to author;
- a programme with **v1 published and assigned, plus a v2 draft** — the immutability rule, made
  visible in a list;
- a programme in **review**, so the lifecycle has a middle;
- an **abandoned** session and an **open** one, not just completed ones;
- a **closed** goal beside open ones;
- an **end-dated** coaching relationship;
- a **voided** invoice and a **part-paid** one;
- a **pending** invitation.

Two properties are non-negotiable:

1. **It goes through the real API.** Nothing is inserted behind the application's back, so
   invitations, capacity grants, immutability triggers and the audit trail are genuinely
   exercised. Seeded data is real data.
2. **It is idempotent.** Every step checks for what it would create and reports "(already
   seeded)". Re-running must be a no-op, not a second set of gyms — an early naive version left
   the demo account a member of six identically-named gyms, which looked exactly like a
   duplicate-rows bug for longer than it should have.

## Alternatives considered

- **SQL fixtures loaded directly.** Faster and much easier to write. Rejected: it bypasses every
  rule the seed is supposed to demonstrate, and would happily create states the API forbids —
  which is worse than no demo, because it makes the product look like it permits them.
- **A `--rich` flag over a minimal default.** Rejected as a false economy. The states above are
  not decoration; they *are* the product. A minimal seed is a seed that misrepresents it.
- **Randomised / faker data.** Rejected: irreproducible demos, and the documented walkthrough in
  [test-accounts.md](../test-accounts.md) would drift from what a reader actually sees.

## Consequences

- **Positive:** a first run of the demo can showcase every built feature, including the
  interesting refusals, with no setup.
- **Positive:** because it drives the real API, the seed doubles as a broad integration smoke
  test — if a rule breaks, seeding fails loudly.
- **Negative:** it is slower than SQL and grows as the product does. Accepted; it runs once.
- **Negative:** the seed must be updated whenever a new invariant lands, or it silently stops
  being a full demonstration. This is the cost of the decision, stated so it is not a surprise.
- **Follow-up:** keep [test-accounts.md](../test-accounts.md) in step — it is the reader's map to
  what the seed produced.

## References

- [ADR-0006: immutable programme versioning](0006-immutable-program-versioning.md)
- [ADR-0019: verification-first development](0019-verification-first-development.md)
- [test-accounts.md](../test-accounts.md)
