-- The transactional outbox and the periodic-job ledger (ADR-0027).
--
-- Two tables, two genuinely different problems, deliberately not merged:
--
--   domain_events   things that HAPPENED, written in the same transaction as
--                   the change they describe, drained once each.
--   job_runs        things that should happen ON A SCHEDULE, whose trigger is
--                   the clock rather than an event.
--
-- Folding the second into the first is the tempting simplification and it is
-- wrong: an outbox row exists because something occurred, and there is no
-- occurrence behind "it is 3am and rent is due". Faking one means writing an
-- event nobody emitted, and then explaining why it has no producer.

-- ------------------------------------------------------------------ outbox

CREATE TABLE domain_events (
    id           UUID        PRIMARY KEY,
    -- Tenant-owned like everything else, so a handler can scope its work.
    gym_id       UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    -- Dotted and past-tense, matching the audit log's vocabulary:
    -- `invoice.issued`, `workout_session.completed`.
    event_type   TEXT        NOT NULL CHECK (length(trim(event_type)) BETWEEN 1 AND 80),
    payload      JSONB       NOT NULL DEFAULT '{}'::jsonb,
    occurred_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Draining state. `processed_at` set means done, once, for good.
    processed_at TIMESTAMPTZ,
    attempts     INT         NOT NULL DEFAULT 0,
    -- Kept so a poison event can be diagnosed instead of just retried forever.
    last_error   TEXT,
    -- Exponential backoff between attempts. NULL means "eligible now".
    next_attempt_at TIMESTAMPTZ,

    CONSTRAINT domain_events_attempts_sane CHECK (attempts >= 0)
);

-- THE index for the drain query. Partial on unprocessed rows, because a busy
-- gym's table is overwhelmingly history and the worker only ever asks about
-- the tail. Ordered by occurred_at so events are handled roughly in the order
-- they happened.
CREATE INDEX domain_events_pending_idx
    ON domain_events (occurred_at)
    WHERE processed_at IS NULL;

-- ------------------------------------------------------------- periodic jobs

-- One row per named job, recording the last time it ran to completion.
--
-- This is what makes "run the billing tick once a day" survive a restart, a
-- crash mid-run, and two workers racing each other. The lock is the row
-- itself: a worker takes `FOR UPDATE` on its job's row for the whole run, so a
-- second worker blocks rather than double-billing anybody.
CREATE TABLE job_runs (
    name          TEXT        PRIMARY KEY CHECK (length(trim(name)) BETWEEN 1 AND 60),
    last_started_at  TIMESTAMPTZ,
    last_finished_at TIMESTAMPTZ,
    -- What the last run did, for the operator reading a log after the fact.
    last_outcome  TEXT,
    -- Consecutive failures. Reset on success; used to stop shouting about a
    -- job that has been broken for a week.
    failures      INT         NOT NULL DEFAULT 0,

    CONSTRAINT job_runs_failures_sane CHECK (failures >= 0)
);

-- ------------------------------------------------- recurring billing, safely

-- The guard that makes the billing tick safe to run twice.
--
-- A nightly job that issues invoices is exactly the kind of thing that gets
-- run twice — a retry, an operator running it by hand, two workers starting
-- together. Every other protection (the job lock, the advance of
-- next_charge_on) is application logic and can be bypassed; this cannot.
-- Double-billing a member is the single worst outcome in this feature, so the
-- database refuses it outright.
--
-- Scoped to invoices that BELONG to a subscription and name a period: ad-hoc
-- charges ("drop-in · guest") legitimately repeat, and voided invoices must
-- not block the corrected re-issue.
CREATE UNIQUE INDEX invoices_one_per_subscription_period
    ON invoices (subscription_id, period_start)
    WHERE subscription_id IS NOT NULL
      AND period_start IS NOT NULL
      AND status <> 'void';

-- Neither table is tenant-scoped by RLS, and that is deliberate.
--
-- `job_runs` has no gym_id — it is deployment-wide infrastructure, not gym
-- data. `domain_events` does have one, but the worker runs with no tenant
-- context by design: its whole job is to sweep across gyms. Both are reached
-- ONLY by the worker's own privileged connection, never by a request handler,
-- which is why the app role gets nothing here.
--
-- If a handler ever needs to read events, that is the moment to add RLS and a
-- grant — not before.
REVOKE ALL ON domain_events FROM gym_app;
REVOKE ALL ON job_runs FROM gym_app;

-- Except: emitting an event is part of the transaction that changes state, so
-- the app role must be able to INSERT. It must NOT be able to mark anything
-- processed, or a request handler could quietly consume the worker's queue.
GRANT INSERT ON domain_events TO gym_app;
