-- Audit log: who did what, to what, in which gym.
--
-- Two properties make this worth having rather than just comforting:
--
-- 1. **Written in the same transaction as the change it records.** A separate
--    write can fail on its own, leaving a mutation with no trace — which is
--    exactly the case an audit log exists for. Atomic or it is decorative.
--
-- 2. **Append-only for the application.** `gym_app` is granted SELECT and INSERT
--    and explicitly denied UPDATE and DELETE, so a bug (or a compromised app
--    role) cannot rewrite history. Only the migration/owner role can, which is
--    the point: tampering requires a second, privileged credential.

CREATE TABLE audit_log (
    id           UUID        PRIMARY KEY,
    gym_id       UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    -- Nullable: some actions have no authenticated actor (system/scheduled work).
    actor_id     UUID        REFERENCES users(id) ON DELETE SET NULL,
    -- Dotted, past-tense, stable: 'exercise.created', 'invitation.accepted'.
    action       TEXT        NOT NULL CHECK (action ~ '^[a-z_]+\.[a-z_]+$'),
    entity_type  TEXT        NOT NULL,
    entity_id    UUID,
    -- Context worth keeping, never anything sensitive: no passwords, no tokens.
    metadata     JSONB       NOT NULL DEFAULT '{}'::jsonb,
    occurred_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The dominant read is "what happened in this gym, most recent first".
CREATE INDEX audit_log_gym_time_idx ON audit_log (gym_id, occurred_at DESC);
-- Secondary: the history of one specific thing.
CREATE INDEX audit_log_entity_idx ON audit_log (gym_id, entity_type, entity_id);

-- Tenant isolation, same as every other tenant-owned table.
ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log FORCE ROW LEVEL SECURITY;

CREATE POLICY audit_log_tenant ON audit_log
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

-- Append-only for the application role. The ALTER DEFAULT PRIVILEGES from the
-- RLS migration granted UPDATE/DELETE on new tables, so take them back here.
REVOKE UPDATE, DELETE ON audit_log FROM gym_app;
GRANT SELECT, INSERT ON audit_log TO gym_app;
