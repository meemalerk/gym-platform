# ADR-0027: A worker, a transactional outbox, and a hand-rolled Postgres queue

- **Status:** Accepted
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Refines:** the "transactional outbox" working principle in CLAUDE.md, and the
  background-jobs note in [docs/research-2026.md](../research-2026.md), which proposed
  apalis + pgmq. This adopts the pattern and declines the dependencies, with reasons.

## Context

CLAUDE.md carried the line *"no `worker` crate yet — it arrives when there are outbox
events to drain"* for the whole project. That was the right call each time it was made.
What forced the issue was not an event but the calendar.

`member_subscriptions.next_charge_on` was written once at signup and never advanced.
`BillingInterval::next_charge_after` was implemented, unit-tested and correct, and had no
caller outside its own tests. So a monthly membership issued exactly one invoice, ever.
Everything a member saw on their Membership screen was true and permanently incomplete,
and no amount of request-handler work could fix it: **nobody makes an HTTP request on the
first of the month.**

Three other things were waiting on the same absence — overdue notices, coach alerts for
clients who have stopped turning up, and eventually push — each deferred with the same
sentence.

## Decision

A `worker` binary, a `domain_events` outbox, and a `job_runs` ledger.

### Two tables, because there are two problems

`domain_events` holds things that **happened**, written in the same transaction as the
change they describe, drained once each. `job_runs` records things that should happen **on
a schedule**, whose trigger is the clock.

Folding the second into the first is the tempting simplification and it is wrong: an
outbox row exists because something occurred, and there is no occurrence behind "it is 3am
and rent is due". Faking one means writing an event nobody emitted and then explaining why
it has no producer.

### No apalis, no pgmq, no Redis

`SELECT ... FOR UPDATE SKIP LOCKED` is the textbook Postgres queue. It is a dozen lines,
it is correct under concurrency, and it needs no infrastructure this project does not
already run. CLAUDE.md's *"don't reach for heavy infra early — this is a gym platform, not
a bank"* applies with force: the research recommendation assumed genuinely independent
consumers, and there is exactly one consumer. The day that changes is the day to revisit
it, and the swap is contained to one module.

The job lease is the same idea applied to periodic work: `FOR UPDATE` on the job's own
row, held for the whole run. Two workers starting in the same second both reach for it,
one wins, the other blocks and then sees a fresh `last_finished_at` and declines. No
leader election, no distributed lock, no clock synchronisation between hosts.

### A separate binary, not a task inside the API

It needs a privileged connection and **no tenant context**, which is the precise opposite
of what every request handler must have — the worker's whole job is to sweep across gyms,
and running it as the RLS-bound `gym_app` role would return nothing from every query while
reporting a clean run. That failure is silent, which is why the worker's `main` says so
out loud. A slow sweep also must not compete with request handling for the pool, and
scaling the API to several instances must not multiply the nightly billing run.

### Three guards against the one thing that matters

Double-billing a member is the worst outcome this feature can produce, and a nightly job
that issues invoices is exactly the kind of thing that gets run twice — a retry, an
operator running it by hand, two workers starting together. So:

1. the **job lease**, so two workers cannot run the tick concurrently;
2. **`next_charge_on` advancing** in the same transaction as the insert;
3. a **unique index** on `(subscription_id, period_start)` for non-void invoices.

The third is the only one that survives a bug in the other two, which is why it exists and
why `verify-worker.sh` asserts it by attempting a duplicate INSERT directly against the
database rather than through the API. The index deliberately excludes voided invoices: a
gym that billed in error must be able to void and re-issue the same period.

### Arrears are settled in one pass

The first version issued one period per subscription per run. A membership three months
behind would therefore take three nights to be told so, showing the member a partial and
wrong balance throughout. The tick now walks forward through every period that has come
due, bounded at 24 so a charge date set to 1970 cannot turn one run into twelve hundred
invoices.

### Noticing is not mutating

`run_overdue_sweep` emits an event and **changes nothing**. "Overdue" is derived from the
due date and today — [the billing migration](../../migrations/20260725000016_billing.sql)
is explicit that storing it would create a second source of truth — and the marker for
"we have already noticed" lives on the *event*, not on the invoice, which is what keeps
that promise. Writing this test proved the point from the other side: the attempt to
back-date an existing invoice as a fixture was refused by `reject_issued_invoice_edit`,
exactly as [ADR-0010](0010-payments-and-billing.md) intends.

## Consequences

**Good.** Recurring billing recurs. Overdue invoices are noticed once. Coaches can be told
that a client has stopped turning up — information nobody can get by looking at a screen,
because the whole point is that nothing happened. Push notifications (UX-2b), dunning and
receipt email are now additions to one `match` arm rather than new infrastructure.

**Cost.** A second process to deploy and watch. A queue that can stall: after
`MAX_ATTEMPTS` an event dead-letters, and the worker then complains about it on every
cycle — loudly and forever, because an event that happened and was never acted on should
be annoying. Nothing is deleted; the row keeps its `last_error` for whoever looks.

**Honest about what is not built.** Every event handler currently logs and succeeds. The
delivery mechanisms these events want — email, push — do not exist. That is the state of
the feature, not a placeholder: what matters is that the events are now produced, recorded
and drained, so a real handler is a change in one place.

**Found along the way**, each by the verification suite rather than by reading code:
`POST /subscriptions` returned only the invoice, so a caller could not reference the
subscription it had just created; `InvoiceResponse` carried no `subscription_id`, so a
member with two memberships could not tell which invoice belonged to which; and there was
**no way to cancel a subscription at all** — which mattered little while billing never
recurred, and became urgent the moment it did.

**Verified by** `scripts/verify-worker.sh` (31 assertions), which covers the three
idempotency guards separately, arrears catch-up, the outbox privilege split (the app role
may INSERT events and may not read, update or consume them), overdue noticing exactly
once, and that cancelling stops the billing.
