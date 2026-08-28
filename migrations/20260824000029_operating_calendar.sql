-- The gym's operating calendar (ADR-0015), built exactly as decided.
--
-- A recurring weekly pattern plus dated overrides, resolved by ONE function.
-- Not three features (schedule, closures, special hours) — they would each
-- need resolving against the others, and the first inconsistency between them
-- is a member turning up to a locked door.
--
-- Times are TIME, never timestamptz. "Opens at 06:00" is a wall-clock fact
-- about a place; storing an instant makes it wrong across DST, silently and
-- seasonally, which is the worst combination to debug.

-- The gym's own clock. An IANA name, not an offset — offsets change twice a
-- year and a stored one is wrong for half of it.
ALTER TABLE gyms
    ADD COLUMN timezone TEXT NOT NULL DEFAULT 'UTC'
        CHECK (length(trim(timezone)) BETWEEN 1 AND 60);

COMMENT ON COLUMN gyms.timezone IS
    'IANA name (e.g. Indian/Maldives). Opening hours are wall-clock times in '
    'this zone, resolved at read time. Never an offset.';

-- ------------------------------------------------------ the weekly pattern

CREATE TABLE gym_opening_hours (
    id        UUID    PRIMARY KEY,
    gym_id    UUID    NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    -- 0 = Sunday, matching Postgres `EXTRACT(DOW)` and JavaScript `getDay()`.
    -- Picking either convention is fine; picking neither and converting in two
    -- places is how a Monday becomes a Sunday.
    weekday   SMALLINT NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    opens_at  TIME    NOT NULL,
    closes_at TIME    NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Several rows per weekday are legal and intended: 06:00-10:00 and
    -- 16:00-22:00 is a split day, which plenty of gyms run.
    CONSTRAINT opening_hours_ordered CHECK (closes_at > opens_at)
);

-- Exact duplicates are a data-entry slip, not a split shift.
CREATE UNIQUE INDEX gym_opening_hours_unique
    ON gym_opening_hours (gym_id, weekday, opens_at, closes_at);
CREATE INDEX gym_opening_hours_lookup ON gym_opening_hours (gym_id, weekday);

-- ------------------------------------------------------------- overrides

CREATE TABLE gym_calendar_overrides (
    id        UUID    PRIMARY KEY,
    gym_id    UUID    NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    on_date   DATE    NOT NULL,

    -- EXPLICIT, never inferred from null hours. "Closed on Eid" and "hours not
    -- configured" are different states, and a system that confuses them either
    -- turns people away or lets them into a locked building.
    is_closed BOOLEAN NOT NULL,
    opens_at  TIME,
    closes_at TIME,
    reason    TEXT    CHECK (reason IS NULL OR length(reason) <= 120),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- One override per date. The resolution rule is "an override wins
    -- entirely"; two of them would need a precedence ladder, which ADR-0015
    -- explicitly refuses.
    UNIQUE (gym_id, on_date),

    CONSTRAINT override_shape CHECK (
        (is_closed AND opens_at IS NULL AND closes_at IS NULL)
        OR (NOT is_closed AND opens_at IS NOT NULL AND closes_at IS NOT NULL
            AND closes_at > opens_at)
    )
);

CREATE INDEX gym_calendar_overrides_lookup
    ON gym_calendar_overrides (gym_id, on_date);

-- ------------------------------------------------- trainer availability

-- Deliberately the IDENTICAL shape to the gym's pattern.
--
-- A trainer's bookable time is the intersection of their availability and the
-- gym's opening hours, and that intersection is only cheap to compute because
-- both sides are the same structure. A different model here — even a "better"
-- one — would make every booking query a translation exercise.
CREATE TABLE trainer_availability (
    id        UUID    PRIMARY KEY,
    gym_id    UUID    NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    trainer_id UUID   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    weekday   SMALLINT NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    opens_at  TIME    NOT NULL,
    closes_at TIME    NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT availability_ordered CHECK (closes_at > opens_at)
);

CREATE UNIQUE INDEX trainer_availability_unique
    ON trainer_availability (gym_id, trainer_id, weekday, opens_at, closes_at);
CREATE INDEX trainer_availability_lookup
    ON trainer_availability (gym_id, trainer_id, weekday);

-- ------------------------------------------------------------------ RLS

-- All three are tenant-owned, so they live under RLS like everything else with
-- a gym_id (ADR-0004). `app_current_gym()` rather than a hand-rolled
-- current_setting: the helper encodes the nullif(...,'') that a committed GUC
-- needs, and every other policy in the schema uses it.
ALTER TABLE gym_opening_hours ENABLE ROW LEVEL SECURITY;
ALTER TABLE gym_opening_hours FORCE ROW LEVEL SECURITY;
CREATE POLICY gym_opening_hours_tenant_isolation ON gym_opening_hours
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE gym_calendar_overrides ENABLE ROW LEVEL SECURITY;
ALTER TABLE gym_calendar_overrides FORCE ROW LEVEL SECURITY;
CREATE POLICY gym_calendar_overrides_tenant_isolation ON gym_calendar_overrides
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE trainer_availability ENABLE ROW LEVEL SECURITY;
ALTER TABLE trainer_availability FORCE ROW LEVEL SECURITY;
CREATE POLICY trainer_availability_tenant_isolation ON trainer_availability
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

-- These three ARE editable and deletable, unlike most of this schema.
--
-- A wrong opening hour is a mistake to correct, not history to preserve: there
-- is no downstream record referencing "we were open 6-10 last Tuesday", and
-- keeping a tombstone for a deleted Wednesday shift would make the resolution
-- rule ("an override wins entirely") stop being one rule.
GRANT SELECT, INSERT, UPDATE, DELETE ON gym_opening_hours TO gym_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON gym_calendar_overrides TO gym_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON trainer_availability TO gym_app;
