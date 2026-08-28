-- Billing: what a gym charges, who is on what, and what was actually owed.
--
-- Scope note (ADR-0010): this is the *gym → member* side — memberships and
-- coaching fees, which both app stores name as IAP-exempt, so they are billed
-- outside a store. The platform → gym SaaS side and the Stripe Connect money
-- movement stay out of this migration; `payments.provider` is the seam they
-- arrive through, and until then a gym records cash and card-on-the-desk.
--
-- The shape is deliberately invoice-first rather than subscription-first: what
-- a gym argues about is a specific charge on a specific date, not an abstract
-- recurrence. Subscriptions produce invoices; invoices are the record.

-- What a gym sells. Price is per interval; `once` is a drop-in.
CREATE TABLE membership_plans (
    id            UUID        PRIMARY KEY,
    gym_id        UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    name          TEXT        NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 80),
    description   TEXT        CHECK (description IS NULL OR length(description) <= 300),
    -- Minor units (pence/cents). Never floats: 0.1 + 0.2 has no place in money.
    price_minor   BIGINT      NOT NULL CHECK (price_minor >= 0),
    currency      TEXT        NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    interval      TEXT        NOT NULL CHECK (interval IN ('monthly', 'once')),
    -- Archived plans keep their subscribers on their existing terms; they only
    -- stop being offered to new ones.
    archived_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (gym_id, name)
);

CREATE INDEX membership_plans_gym_idx ON membership_plans (gym_id) WHERE archived_at IS NULL;

-- Who is on what. The price is COPIED from the plan at signup: a plan's price
-- change must not silently re-price everyone already on it (and must never
-- alter what an issued invoice said).
CREATE TABLE member_subscriptions (
    id             UUID        PRIMARY KEY,
    gym_id         UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    member_id      UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    plan_id        UUID        NOT NULL REFERENCES membership_plans(id) ON DELETE RESTRICT,
    price_minor    BIGINT      NOT NULL CHECK (price_minor >= 0),
    currency       TEXT        NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    status         TEXT        NOT NULL CHECK (status IN ('active', 'cancelled')),
    started_on     DATE        NOT NULL,
    -- The next date an invoice is due. Null for a one-off.
    next_charge_on DATE,
    cancelled_at   TIMESTAMPTZ,
    -- Access runs to the end of the paid period, so cancelling names a date.
    ends_on        DATE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT subscription_status_evidence CHECK (
        (status <> 'cancelled' OR cancelled_at IS NOT NULL)
        AND (status <> 'active' OR cancelled_at IS NULL)
    )
);

-- One active subscription per member per plan; cancelled ones may repeat.
CREATE UNIQUE INDEX member_subscriptions_active_idx
    ON member_subscriptions (gym_id, member_id, plan_id)
    WHERE status = 'active';
CREATE INDEX member_subscriptions_member_idx ON member_subscriptions (gym_id, member_id);

-- What was owed, for what, when. An issued invoice is a statement of fact and
-- is never rewritten — corrections are a void plus a new invoice.
CREATE TABLE invoices (
    id              UUID        PRIMARY KEY,
    gym_id          UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    member_id       UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    subscription_id UUID        REFERENCES member_subscriptions(id) ON DELETE SET NULL,
    -- Human reference, unique within the gym: "INV-2026-0142".
    reference       TEXT        NOT NULL CHECK (length(trim(reference)) BETWEEN 1 AND 40),
    description     TEXT        NOT NULL CHECK (length(trim(description)) BETWEEN 1 AND 120),
    amount_minor    BIGINT      NOT NULL CHECK (amount_minor >= 0),
    currency        TEXT        NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    -- The service period this covers. Both null for a one-off charge.
    period_start    DATE,
    period_end      DATE,
    issued_on       DATE        NOT NULL,
    due_on          DATE        NOT NULL,
    status          TEXT        NOT NULL CHECK (status IN ('due', 'paid', 'void')),
    paid_at         TIMESTAMPTZ,
    voided_at       TIMESTAMPTZ,
    voided_by       UUID        REFERENCES users(id),
    void_reason     TEXT        CHECK (void_reason IS NULL OR length(void_reason) <= 200),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (gym_id, reference),
    CONSTRAINT invoice_period_ordered CHECK (
        period_start IS NULL OR period_end IS NULL OR period_end >= period_start
    ),
    CONSTRAINT invoice_due_after_issue CHECK (due_on >= issued_on),
    -- "Overdue" is NOT a stored state: it is `due` and past `due_on`, computed
    -- on read. A status that depends on today cannot be stored without a job to
    -- keep it true, and that job is a second source of truth.
    CONSTRAINT invoice_status_evidence CHECK (
        (status <> 'paid' OR paid_at IS NOT NULL)
        AND (status <> 'void' OR (voided_at IS NOT NULL AND voided_by IS NOT NULL))
        AND (status <> 'due' OR (paid_at IS NULL AND voided_at IS NULL))
    )
);

CREATE INDEX invoices_gym_idx ON invoices (gym_id, issued_on DESC);
CREATE INDEX invoices_member_idx ON invoices (gym_id, member_id, issued_on DESC);
CREATE INDEX invoices_outstanding_idx ON invoices (gym_id, due_on) WHERE status = 'due';

-- Money received against an invoice. Append-only: a mistaken payment is
-- corrected by a refund row, never by deleting the record of what happened.
CREATE TABLE payments (
    id           UUID        PRIMARY KEY,
    gym_id       UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    invoice_id   UUID        NOT NULL REFERENCES invoices(id) ON DELETE RESTRICT,
    amount_minor BIGINT      NOT NULL CHECK (amount_minor <> 0),
    currency     TEXT        NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    -- The seam Stripe arrives through. `cash` and `card_terminal` are what a
    -- gym records by hand today; `stripe` will carry a provider reference.
    provider     TEXT        NOT NULL CHECK (provider IN ('cash', 'card_terminal', 'stripe')),
    provider_ref TEXT        CHECK (provider_ref IS NULL OR length(provider_ref) <= 120),
    received_on  DATE        NOT NULL,
    recorded_by  UUID        NOT NULL REFERENCES users(id),
    note         TEXT        CHECK (note IS NULL OR length(note) <= 200),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX payments_invoice_idx ON payments (invoice_id);
CREATE INDEX payments_gym_idx ON payments (gym_id, received_on DESC);

-- An issued invoice's terms are frozen. Only its lifecycle may move, and only
-- forwards: void and paid are terminal.
CREATE OR REPLACE FUNCTION reject_issued_invoice_edit() RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status <> 'due' AND NEW.status <> OLD.status THEN
        RAISE EXCEPTION 'invoice % is % and cannot change state', OLD.reference, OLD.status;
    END IF;

    IF NEW.amount_minor <> OLD.amount_minor
       OR NEW.currency   <> OLD.currency
       OR NEW.member_id  <> OLD.member_id
       OR NEW.reference  <> OLD.reference
       OR NEW.issued_on  <> OLD.issued_on
       OR NEW.description <> OLD.description
       OR NEW.period_start IS DISTINCT FROM OLD.period_start
       OR NEW.period_end   IS DISTINCT FROM OLD.period_end
    THEN
        RAISE EXCEPTION 'an issued invoice cannot be rewritten; void it and issue another';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER invoices_frozen
    BEFORE UPDATE ON invoices
    FOR EACH ROW EXECUTE FUNCTION reject_issued_invoice_edit();

-- ---------------------------------------------------------------- tenancy

ALTER TABLE membership_plans     ENABLE ROW LEVEL SECURITY;
ALTER TABLE membership_plans     FORCE  ROW LEVEL SECURITY;
ALTER TABLE member_subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE member_subscriptions FORCE  ROW LEVEL SECURITY;
ALTER TABLE invoices             ENABLE ROW LEVEL SECURITY;
ALTER TABLE invoices             FORCE  ROW LEVEL SECURITY;
ALTER TABLE payments             ENABLE ROW LEVEL SECURITY;
ALTER TABLE payments             FORCE  ROW LEVEL SECURITY;

CREATE POLICY membership_plans_tenant_isolation ON membership_plans
    USING (gym_id = app_current_gym()) WITH CHECK (gym_id = app_current_gym());
CREATE POLICY member_subscriptions_tenant_isolation ON member_subscriptions
    USING (gym_id = app_current_gym()) WITH CHECK (gym_id = app_current_gym());
CREATE POLICY invoices_tenant_isolation ON invoices
    USING (gym_id = app_current_gym()) WITH CHECK (gym_id = app_current_gym());
CREATE POLICY payments_tenant_isolation ON payments
    USING (gym_id = app_current_gym()) WITH CHECK (gym_id = app_current_gym());

-- Narrow grants, then explicit revokes: migration 0003's ALTER DEFAULT
-- PRIVILEGES hands `gym_app` everything on every new table, so a GRANT here is
-- additive and a REVOKE is the only thing that actually removes anything —
-- the lesson of migration 0012.
GRANT SELECT, INSERT, UPDATE ON membership_plans     TO gym_app;
GRANT SELECT, INSERT, UPDATE ON member_subscriptions TO gym_app;
GRANT SELECT, INSERT, UPDATE ON invoices             TO gym_app;
GRANT SELECT, INSERT         ON payments             TO gym_app;

REVOKE DELETE ON membership_plans     FROM gym_app;
REVOKE DELETE ON member_subscriptions FROM gym_app;
REVOKE DELETE ON invoices             FROM gym_app;
-- A financial record is never edited or removed by the application. A refund
-- is another row; a correction is another row.
REVOKE UPDATE, DELETE ON payments     FROM gym_app;
