-- Programmes: the heart of the product (ADR-0006).
--
-- A programme holds no training content. Content lives in versions, and a
-- PUBLISHED VERSION IS IMMUTABLE — that is the invariant everything else here
-- defends. A member assigned v2 must be able to see, a year later, exactly what
-- v2 told them to do.
--
-- The domain layer enforces the lifecycle, but the database enforces it too. The
-- application is not the only thing that can reach these tables, and "we always
-- check in Rust" is exactly the assumption that stops being true the first time
-- someone writes a migration or a maintenance script.

-- ---------------------------------------------------------------- programmes

CREATE TABLE programs (
    id          UUID        PRIMARY KEY,
    gym_id      UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    name        TEXT        NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 160),
    summary     TEXT        CHECK (summary IS NULL OR length(summary) <= 2000),
    created_by  UUID        NOT NULL REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX programs_gym_idx ON programs (gym_id);
CREATE UNIQUE INDEX programs_gym_name_unique ON programs (gym_id, lower(name));

-- ------------------------------------------------------------------ versions

CREATE TABLE program_versions (
    id              UUID        PRIMARY KEY,
    program_id      UUID        NOT NULL REFERENCES programs(id) ON DELETE CASCADE,
    -- Denormalised from programs so every RLS policy below can be a plain column
    -- comparison instead of a subquery. Tenancy checks run on every single row
    -- read; they must not need a join.
    gym_id          UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    version_number  INTEGER     NOT NULL CHECK (version_number >= 1),
    status          TEXT        NOT NULL
        CHECK (status IN ('draft','in_review','approved','published','archived')),

    -- Each state's evidence. Mirrors the VersionStatus enum in the domain, where
    -- these are variant fields rather than nullable columns.
    submitted_at    TIMESTAMPTZ,
    submitted_by    UUID REFERENCES users(id),
    approved_at     TIMESTAMPTZ,
    approved_by     UUID REFERENCES users(id),
    published_at    TIMESTAMPTZ,
    published_by    UUID REFERENCES users(id),
    archived_at     TIMESTAMPTZ,
    archived_by     UUID REFERENCES users(id),

    created_by      UUID        NOT NULL REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The published version this draft was branched from, so lineage stays visible.
    derived_from    UUID REFERENCES program_versions(id),

    -- The nullable columns above are the price of storing a sum type in SQL. This
    -- constraint buys back what the Rust enum gives for free: a published row
    -- without a publisher is not representable.
    CONSTRAINT program_versions_status_evidence CHECK (
        (status <> 'published' OR (published_at IS NOT NULL AND published_by IS NOT NULL))
        AND (status <> 'approved'  OR (approved_at  IS NOT NULL AND approved_by  IS NOT NULL))
        AND (status <> 'in_review' OR (submitted_at IS NOT NULL AND submitted_by IS NOT NULL))
        AND (status <> 'archived'  OR (archived_at  IS NOT NULL AND archived_by  IS NOT NULL))
    )
);

CREATE INDEX program_versions_program_idx ON program_versions (program_id, version_number DESC);
CREATE INDEX program_versions_gym_idx ON program_versions (gym_id);

CREATE UNIQUE INDEX program_versions_number_unique
    ON program_versions (program_id, version_number);

-- At most one version of a programme may be open for editing at a time.
-- Otherwise two coaches each start "the next version" and silently diverge.
CREATE UNIQUE INDEX program_versions_one_open_draft
    ON program_versions (program_id)
    WHERE status IN ('draft', 'in_review', 'approved');

-- --------------------------------------------------------------- the content

CREATE TABLE program_weeks (
    id          UUID        PRIMARY KEY,
    version_id  UUID        NOT NULL REFERENCES program_versions(id) ON DELETE CASCADE,
    gym_id      UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    week_number INTEGER     NOT NULL CHECK (week_number BETWEEN 1 AND 52),
    label       TEXT        CHECK (label IS NULL OR length(label) <= 120)
);

CREATE UNIQUE INDEX program_weeks_unique ON program_weeks (version_id, week_number);
CREATE INDEX program_weeks_gym_idx ON program_weeks (gym_id);

CREATE TABLE workout_templates (
    id          UUID        PRIMARY KEY,
    week_id     UUID        NOT NULL REFERENCES program_weeks(id) ON DELETE CASCADE,
    gym_id      UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    day_number  INTEGER     NOT NULL CHECK (day_number BETWEEN 1 AND 14),
    name        TEXT        NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 120),
    notes       TEXT        CHECK (notes IS NULL OR length(notes) <= 2000)
);

CREATE UNIQUE INDEX workout_templates_unique ON workout_templates (week_id, day_number);
CREATE INDEX workout_templates_gym_idx ON workout_templates (gym_id);

CREATE TABLE workout_template_exercises (
    id           UUID        PRIMARY KEY,
    workout_id   UUID        NOT NULL REFERENCES workout_templates(id) ON DELETE CASCADE,
    gym_id       UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    -- RESTRICT, not CASCADE: deleting a catalogue entry must never silently
    -- remove movements from a published plan. The delete fails and the coach is
    -- told why, which is the honest outcome.
    exercise_id  UUID        NOT NULL REFERENCES exercises(id) ON DELETE RESTRICT,
    position     INTEGER     NOT NULL CHECK (position >= 1),
    -- The prescription as a tagged union, exactly as the domain enum serialises.
    -- JSONB because its shape depends on the variant; the CHECK keeps the tag
    -- honest so a row can never claim a kind the application cannot parse.
    prescription JSONB       NOT NULL
        CHECK (prescription->>'kind' IN ('repetitions','duration','distance')),
    notes        TEXT        CHECK (notes IS NULL OR length(notes) <= 1000)
);

CREATE UNIQUE INDEX workout_template_exercises_position_unique
    ON workout_template_exercises (workout_id, position);
CREATE INDEX workout_template_exercises_gym_idx ON workout_template_exercises (gym_id);
CREATE INDEX workout_template_exercises_exercise_idx
    ON workout_template_exercises (exercise_id);

-- ------------------------------------------------- immutability, in the database

-- The domain refuses to edit a frozen version. This makes the database refuse
-- too, so a bug, a maintenance script or a future endpoint cannot quietly
-- rewrite training history. Belt and braces, deliberately: this invariant is the
-- one the entire product rests on.
CREATE OR REPLACE FUNCTION reject_frozen_version_content()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    target_version UUID;
    current_status TEXT;
BEGIN
    target_version := CASE TG_TABLE_NAME
        WHEN 'program_weeks' THEN COALESCE(NEW.version_id, OLD.version_id)
        WHEN 'workout_templates' THEN (
            SELECT version_id FROM program_weeks
            WHERE id = COALESCE(NEW.week_id, OLD.week_id)
        )
        ELSE (
            SELECT w.version_id
            FROM workout_templates t
            JOIN program_weeks w ON w.id = t.week_id
            WHERE t.id = COALESCE(NEW.workout_id, OLD.workout_id)
        )
    END;

    SELECT status INTO current_status
    FROM program_versions WHERE id = target_version;

    -- A cascading delete of the whole version has already removed the parent row;
    -- that is teardown, not an edit, so let it through.
    IF current_status IS NULL THEN
        RETURN COALESCE(NEW, OLD);
    END IF;

    IF current_status <> 'draft' THEN
        RAISE EXCEPTION
            'cannot modify content of a % program version', current_status
            USING ERRCODE = 'raise_exception';
    END IF;

    RETURN COALESCE(NEW, OLD);
END;
$$;

CREATE TRIGGER program_weeks_frozen
    BEFORE INSERT OR UPDATE OR DELETE ON program_weeks
    FOR EACH ROW EXECUTE FUNCTION reject_frozen_version_content();

CREATE TRIGGER workout_templates_frozen
    BEFORE INSERT OR UPDATE OR DELETE ON workout_templates
    FOR EACH ROW EXECUTE FUNCTION reject_frozen_version_content();

CREATE TRIGGER workout_template_exercises_frozen
    BEFORE INSERT OR UPDATE OR DELETE ON workout_template_exercises
    FOR EACH ROW EXECUTE FUNCTION reject_frozen_version_content();

-- A published version's own row is frozen except for the archive transition.
-- Publishing is the last edit; archiving is a retirement flag, not a content change.
CREATE OR REPLACE FUNCTION reject_published_version_edit()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status = 'published' AND NEW.status <> 'archived' THEN
        RAISE EXCEPTION 'a published program version is immutable'
            USING ERRCODE = 'raise_exception';
    END IF;
    IF OLD.status = 'archived' THEN
        RAISE EXCEPTION 'an archived program version cannot be changed'
            USING ERRCODE = 'raise_exception';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER program_versions_immutable
    BEFORE UPDATE ON program_versions
    FOR EACH ROW EXECUTE FUNCTION reject_published_version_edit();

-- ------------------------------------------------------------------------ RLS

ALTER TABLE programs ENABLE ROW LEVEL SECURITY;
ALTER TABLE programs FORCE ROW LEVEL SECURITY;
CREATE POLICY programs_tenant_isolation ON programs
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE program_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE program_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY program_versions_tenant_isolation ON program_versions
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE program_weeks ENABLE ROW LEVEL SECURITY;
ALTER TABLE program_weeks FORCE ROW LEVEL SECURITY;
CREATE POLICY program_weeks_tenant_isolation ON program_weeks
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE workout_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE workout_templates FORCE ROW LEVEL SECURITY;
CREATE POLICY workout_templates_tenant_isolation ON workout_templates
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE workout_template_exercises ENABLE ROW LEVEL SECURITY;
ALTER TABLE workout_template_exercises FORCE ROW LEVEL SECURITY;
CREATE POLICY workout_template_exercises_tenant_isolation ON workout_template_exercises
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

GRANT SELECT, INSERT, UPDATE, DELETE ON programs TO gym_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON program_versions TO gym_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON program_weeks TO gym_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON workout_templates TO gym_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON workout_template_exercises TO gym_app;
