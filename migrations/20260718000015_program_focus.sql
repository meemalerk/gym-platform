-- Programme focus: what a programme is FOR, in one coach-chosen word.
--
-- This is the metadata that makes recommendations a deterministic rule ("a lift
-- goal suggests strength programmes") instead of a model guessing from prose.
-- Existing programmes default to 'general', which recommends nothing — honest,
-- until a coach says otherwise.

ALTER TABLE programs
    ADD COLUMN focus TEXT NOT NULL DEFAULT 'general'
        CHECK (focus IN ('strength','hypertrophy','conditioning','general'));
