-- Stripe arrives (ADR-0010's seam, finally used): self-service card payment
-- for a member's own invoice, via Stripe Checkout + a verified webhook.
--
-- This migration adds exactly one thing: a database-level backstop for
-- webhook idempotency. `BillingService::apply_stripe_payment` already checks
-- for an existing payment with the same `provider_ref` before inserting, but
-- Stripe redelivers webhooks, two deliveries can race each other past that
-- check, and "the app always checks first" is precisely the assumption ADR-0004
-- exists to not rely on alone. A unique index makes a double-credit a
-- constraint violation instead of a race — belt and braces, same as the
-- program-immutability triggers (migration `20260718000007`).
--
-- Scoped to `provider = 'stripe'` only: cash and card-terminal payments reuse
-- `provider_ref` as a free-text till slip number today, which is not
-- guaranteed unique and was never meant to be.
CREATE UNIQUE INDEX payments_stripe_ref_unique
    ON payments (gym_id, provider_ref)
    WHERE provider = 'stripe' AND provider_ref IS NOT NULL;
