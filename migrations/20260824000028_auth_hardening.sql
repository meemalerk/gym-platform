-- Password reset, email verification, login throttling, and outbound mail
-- (ADR-0029).
--
-- Three holes, one migration, because they share a shape: each needs a
-- single-use secret that is emailed and never stored in the clear, and none of
-- them could exist while the platform had no way to send an email at all.
--
-- Before this: a forgotten password locked an account out permanently — there
-- was no reset of any kind — sign-up accepted any address without checking
-- anyone could read it, and the login endpoint counted nothing, so an attacker
-- could try passwords as fast as the network allowed.

-- ------------------------------------------------------- single-use secrets

-- One table for both flows, distinguished by `purpose`.
--
-- They are the same object: a hashed, expiring, single-use token bound to one
-- account. Two tables would be two sets of the same bugs — and the invitation
-- table (migration 0002) already proved this shape works, so this follows it
-- deliberately rather than inventing a third.
CREATE TABLE auth_tokens (
    id         UUID        PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose    TEXT        NOT NULL CHECK (purpose IN ('password_reset', 'email_verification')),

    -- **Only the hash.** Same rule as refresh tokens and invitations: a
    -- database leak must not hand over live credentials. The raw value exists
    -- once, in the email, and nowhere else.
    token_hash BYTEA       NOT NULL,

    -- Bound to the address it was sent to, so a forwarded link is useless.
    -- Denormalised on purpose: if the account changes its email between issue
    -- and redemption, the old link must stop working.
    email      TEXT        NOT NULL,

    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (token_hash)
);

-- The redemption lookup, and nothing else. A token is found by its hash or not
-- at all — there is deliberately no way to list a user's live tokens, because
-- no legitimate caller needs one and it would be a fine thing for an attacker
-- to have.
CREATE INDEX auth_tokens_user_purpose_idx
    ON auth_tokens (user_id, purpose)
    WHERE used_at IS NULL;

-- Has this account proved it can read its email?
--
-- Nullable rather than a boolean: "when" is strictly more information than
-- "whether", costs the same, and a support conversation about an account
-- always wants the date.
ALTER TABLE users
    ADD COLUMN email_verified_at TIMESTAMPTZ;

COMMENT ON COLUMN users.email_verified_at IS
    'When the account proved it can read its address. NULL = unverified. '
    'Deliberately does NOT block sign-in — see ADR-0029.';

-- ----------------------------------------------------------- login throttle

-- Every login attempt, successful or not.
--
-- In the database rather than in memory, for two reasons that both matter: a
-- restart must not reset an attacker's budget, and a second API instance must
-- not double it. The cost is one insert per login, on a table nothing else
-- reads.
CREATE TABLE login_attempts (
    id           BIGSERIAL   PRIMARY KEY,
    -- The email as TYPED, lowercased. Not a user_id: the whole point is to
    -- throttle attempts against addresses that may not exist, and resolving to
    -- an id first would leak which ones do.
    email        TEXT        NOT NULL,
    -- Best-effort. Behind a proxy this is whatever the deployment forwards,
    -- which is why it is a secondary signal and never the only one.
    ip           TEXT,
    succeeded    BOOLEAN     NOT NULL,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The two counting queries. Partial on failures because successes are never
-- counted — a person logging in repeatedly from three devices is not an
-- attack, and throttling them would be.
CREATE INDEX login_attempts_email_idx
    ON login_attempts (email, attempted_at DESC)
    WHERE NOT succeeded;
CREATE INDEX login_attempts_ip_idx
    ON login_attempts (ip, attempted_at DESC)
    WHERE NOT succeeded AND ip IS NOT NULL;

-- ---------------------------------------------------------------- outbound mail

-- Every message the platform sent.
--
-- Not a queue: the outbox (migration 20260824000026) is the queue. This is the
-- RECORD, and it exists for three reasons — a support question ("did the reset
-- email go?") has an answer, the demo can show the mail it would have sent
-- without configuring a provider, and the verification suite can read the link
-- it is supposed to follow.
--
-- The body is stored, which is a deliberate and slightly uncomfortable choice:
-- it means a reset LINK sits in the database for as long as the row does. That
-- is acceptable only because the token it contains is itself single-use and
-- short-lived, and because the alternative — an unauditable mail path — is
-- worse for a system that sends credentials. `scripts` prune this in a real
-- deployment; there is no retention job yet and that is called out in the ADR.
CREATE TABLE sent_emails (
    id         UUID        PRIMARY KEY,
    to_email   TEXT        NOT NULL,
    subject    TEXT        NOT NULL,
    body       TEXT        NOT NULL,
    -- `password_reset`, `email_verification`, … so a reader can filter without
    -- pattern-matching on the subject line.
    kind       TEXT        NOT NULL,
    sent_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sent_emails_recent_idx ON sent_emails (sent_at DESC);

-- None of these four tables is tenant-owned: an account, its credentials and
-- the mail sent to it all exist BEFORE any gym is involved, and a password
-- reset must work for someone who belongs to no gym at all. They sit outside
-- RLS for the same reason `users` and `sessions` do — the service layer is the
-- only wall, which is why every query here filters by user id or token hash.
--
-- The app role needs full access: unlike the outbox, these are written and
-- read by request handlers.
GRANT SELECT, INSERT, UPDATE ON auth_tokens TO gym_app;
GRANT SELECT, INSERT ON login_attempts TO gym_app;
GRANT USAGE, SELECT ON SEQUENCE login_attempts_id_seq TO gym_app;
GRANT SELECT, INSERT ON sent_emails TO gym_app;

-- Append-only, both of them. An attempt log you can edit is not evidence, and
-- a record of sent mail you can delete cannot answer the question it exists for.
REVOKE DELETE, UPDATE ON login_attempts FROM gym_app;
REVOKE DELETE, UPDATE ON sent_emails FROM gym_app;
-- A token is marked used, never removed: "this link was already used" is a
-- more useful answer than "this link does not exist", and keeping the row is
-- what allows it.
REVOKE DELETE ON auth_tokens FROM gym_app;
