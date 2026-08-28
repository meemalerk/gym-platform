-- Trainers may name movements (ADR-0024), so the catalogue gains curation.
--
-- Why this is not just "let trainers INSERT": progress is computed per
-- exercise_id (ADR-0018). Two rows for one movement permanently splits an
-- athlete's estimated-1RM history into two half-charts, and performed_sets
-- already point at the ids, so no later edit can rejoin them. `status` is what
-- lets a curator catch the duplicate before it spreads, WITHOUT blocking the
-- trainer who is mid-programme — a proposal is prescribable from the moment it
-- exists.
--
-- `retired` is the third state and it is deliberately not a DELETE: performed
-- sets reference exercises with ON DELETE RESTRICT, and history does not stop
-- being true because the gym stopped programming the movement.

ALTER TABLE exercises
    ADD COLUMN status TEXT NOT NULL DEFAULT 'approved'
        CHECK (status IN ('proposed', 'approved', 'retired')),
    -- Nullable, and it stays nullable: every row that existed before this
    -- migration was written by a catalogue manager whose id we did not record
    -- at the time. Back-filling a guess would be worse than an honest NULL —
    -- the curation queue reads it as "nobody to chase", which is correct.
    ADD COLUMN proposed_by UUID REFERENCES users(id) ON DELETE SET NULL;

-- The curation queue is the only hot read this adds: "what is waiting for me?"
-- Partial, because approved rows are the overwhelming majority and a curator
-- never queries for them by status.
CREATE INDEX exercises_pending_curation_idx
    ON exercises (gym_id, name)
    WHERE status = 'proposed';

COMMENT ON COLUMN exercises.status IS
    'proposed = raised by a coach and usable by them, awaiting curation; '
    'approved = part of the gym vocabulary; '
    'retired = not offered for NEW prescriptions, existing ones untouched.';

COMMENT ON COLUMN exercises.proposed_by IS
    'Who raised it. NULL for rows predating ADR-0024 — not back-filled, '
    'because a guessed author is worse than an honest absence.';
