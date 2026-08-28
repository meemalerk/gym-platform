-- Revoke what the default privileges silently granted.
--
-- Migration 0003 sets ALTER DEFAULT PRIVILEGES so every new table gives gym_app
-- full DML. That is the right default for ordinary tables, but it means a
-- "GRANT SELECT, INSERT" on a history table is ADDITIVE, not restrictive — the
-- table still carries UPDATE and DELETE from the default. The audit log's
-- migration (0005) knew this and revoked; three later tables did not:
--
--   coach_relationships   claimed "no DELETE grant" — had DELETE
--   program_assignments   claimed "no DELETE grant" — had DELETE
--   performed_sets        claimed INSERT+SELECT only — had UPDATE and DELETE
--   workout_sessions      end-dated, never deleted — had DELETE
--
-- Caught by verify-execution.sh asserting the privilege directly instead of
-- trusting the migration's comment. The lesson is recorded here so the next
-- history table's author greps for this file: **a narrow GRANT is not a
-- revocation. Revoke explicitly.**

REVOKE UPDATE, DELETE ON performed_sets FROM gym_app;
REVOKE DELETE ON workout_sessions FROM gym_app;
REVOKE DELETE ON coach_relationships FROM gym_app;
REVOKE DELETE ON program_assignments FROM gym_app;
