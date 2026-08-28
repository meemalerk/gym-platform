-- An honest workout duration (ADR-0008's clock discipline, applied to the end
-- of a session as well as the start).
--
-- `started_at` already travels with the data: the phone records when the
-- athlete actually began, and syncs whenever it can. The END did not. Finishing
-- a session set `completed_at` from the SERVER clock at the moment the request
-- arrived — so a workout logged offline and synced three hours later recorded a
-- three-hour-long training session. Every duration, average and "how long did
-- they train" a coach reads was wrong by however long the phone was offline.
--
-- Both times are kept, because they answer different questions:
--
--   ended_at      the athlete's clock. What duration is computed from.
--   completed_at  our clock. When we were told. Still the audit fact, and
--                 still what the status enum carries as its evidence.
--
-- Nullable and NOT back-filled: sessions recorded before this have no honest
-- end time, and inventing one would be worse than the fallback the domain
-- already does (use completed_at, and be approximately right about history
-- that is mostly online anyway).
ALTER TABLE workout_sessions
    ADD COLUMN ended_at TIMESTAMPTZ;

-- The end cannot precede the start. The domain drops an impossible client value
-- rather than erroring — an athlete must always be able to close a workout,
-- even with a drifting phone clock — so nothing should ever reach this. That is
-- exactly why it is worth asserting: if it ever fires, the drop-on-invalid path
-- has been bypassed.
ALTER TABLE workout_sessions
    ADD CONSTRAINT workout_sessions_end_after_start
    CHECK (ended_at IS NULL OR ended_at >= started_at);

COMMENT ON COLUMN workout_sessions.ended_at IS
    'Client wall-clock end, paired with started_at. Durations are computed from '
    'these two; completed_at is the server''s record of when it heard, and is '
    'hours late for anything synced offline.';
