-- Invoice numbering that cannot collide.
--
-- The first implementation read every invoice, took the highest reference and
-- added one — outside the transaction that then inserted. Two managers issuing
-- at the same moment computed the same number, and the second lost to
-- UNIQUE (gym_id, reference) with a conflict that named nothing a person could
-- act on. Money code does not get to have a race.
--
-- This is a per-gym, per-year counter incremented atomically by the same
-- statement that reads it, so two callers can never be handed the same number.
--
-- Gaps remain possible — a number is taken, then its invoice fails to insert —
-- and that is the correct trade. A gap is a question an accountant can answer
-- ("that one was rolled back"); a duplicate invoice number is a question nobody
-- can answer.

CREATE TABLE invoice_sequences (
    gym_id      UUID NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    year        INT  NOT NULL CHECK (year BETWEEN 2000 AND 9999),
    -- The number the NEXT invoice will take.
    next_number INT  NOT NULL CHECK (next_number > 0),

    PRIMARY KEY (gym_id, year)
);

-- Backfill from what already exists, so numbering continues rather than
-- restarting at 1 and colliding with invoices issued before this migration.
INSERT INTO invoice_sequences (gym_id, year, next_number)
SELECT gym_id,
       CAST(substring(reference FROM 5 FOR 4) AS INT) AS ref_year,
       MAX(CAST(substring(reference FROM 10) AS INT)) + 1
  FROM invoices
 WHERE reference ~ '^INV-[0-9]{4}-[0-9]+$'
 GROUP BY gym_id, ref_year;

ALTER TABLE invoice_sequences ENABLE ROW LEVEL SECURITY;
ALTER TABLE invoice_sequences FORCE  ROW LEVEL SECURITY;

CREATE POLICY invoice_sequences_tenant_isolation ON invoice_sequences
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

GRANT SELECT, INSERT, UPDATE ON invoice_sequences TO gym_app;
-- Never: deleting a counter would let a gym re-issue numbers it has already used.
REVOKE DELETE ON invoice_sequences FROM gym_app;
