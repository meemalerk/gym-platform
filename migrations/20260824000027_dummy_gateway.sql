-- A card gateway this deployment serves itself (ADR-0028).
--
-- ADR-0010 chose Stripe and the seam was built for it, but a Stripe key is a
-- real commercial account: it cannot be part of a clone-and-run demo, and it
-- cannot be part of a verification suite. So the whole payment path — redirect,
-- pay, return, settle, retry — was the one significant flow with no executable
-- proof, in a codebase whose entire discipline is executable proof (ADR-0019).
--
-- `dummy` is not a test double. It is a real implementation of the same
-- `PaymentGateway` port, hosting its own card page, with the bank replaced by
-- a rule about which card numbers succeed. Everything downstream of it — the
-- payment row, the settle, the idempotency key — is the production path.
--
-- It gets its own provider value rather than reusing 'stripe' precisely so a
-- row in the money table can never be mistaken for a real one.

ALTER TABLE payments
    DROP CONSTRAINT payments_provider_check;

ALTER TABLE payments
    ADD CONSTRAINT payments_provider_check
    CHECK (provider IN ('cash', 'card_terminal', 'stripe', 'dummy'));

-- The redelivery guard, generalised.
--
-- `payments_stripe_ref_unique` (migration 20260823000020) protects against a
-- webhook arriving twice. The dummy gateway can be double-submitted in exactly
-- the same way — a refreshed browser, a double-tapped button — and needs the
-- same protection. Cash and card-terminal payments still reuse provider_ref as
-- a free-text till slip number, which is not unique and was never meant to be,
-- so they stay outside it.
DROP INDEX IF EXISTS payments_stripe_ref_unique;

CREATE UNIQUE INDEX payments_gateway_ref_unique
    ON payments (gym_id, provider, provider_ref)
    WHERE provider IN ('stripe', 'dummy') AND provider_ref IS NOT NULL;

COMMENT ON INDEX payments_gateway_ref_unique IS
    'One payment per gateway reference. The application checks first; this is '
    'what makes a double-credit a constraint violation rather than a race.';
