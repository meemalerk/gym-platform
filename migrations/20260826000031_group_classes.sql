-- Group classes, and booking a place in one.
--
-- Two tables because there are genuinely two things, on different clocks:
--
--   gym_classes    the RECURRING SLOT — "Zumba, Mondays at 18:00, cap 20".
--                  A fact about the timetable. Changes rarely.
--   class_bookings one member in ONE OCCURRENCE of that slot — "this Monday".
--                  A fact about a person's week. Changes constantly.
--
-- Collapsing them (a row per class per week, generated ahead) was the obvious
-- alternative and is worse: it needs a job to mint future rows, it has to
-- decide how far ahead to mint them, and editing the timetable then means
-- rewriting every unheld occurrence. Deriving occurrences from the weekly slot
-- at read time has none of those problems — the timetable is the source of
-- truth and an occurrence is just (slot, date).
--
-- Times are TIME and weekday is 0=Sunday, matching gym_opening_hours exactly.
-- A class is bounded by the gym's opening hours, and that comparison is only
-- cheap because both sides are the same shape (see the operating-calendar
-- migration's note on trainer_availability, which made the same choice).

-- ------------------------------------------------------- the weekly slot

CREATE TABLE gym_classes (
    id               UUID     PRIMARY KEY,
    gym_id           UUID     NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    name             TEXT     NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 80),
    -- Who teaches it. NOT NULL on purpose: "a class with no instructor" is a
    -- draft the timetable should not be showing, and the trainer's own
    -- dashboard ("your classes today") has nothing to key on without it.
    instructor_id    UUID     NOT NULL REFERENCES users(id),
    -- 0 = Sunday. Same convention as gym_opening_hours, EXTRACT(DOW) and
    -- JavaScript getDay(); see that migration for why picking one matters.
    weekday          SMALLINT NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    starts_at        TIME     NOT NULL,
    -- Duration rather than an end time. A class is "45 minutes of Zumba", and
    -- storing the end invites a row where the end precedes the start.
    duration_minutes SMALLINT NOT NULL CHECK (duration_minutes BETWEEN 5 AND 300),
    -- The physical limit: mats, bikes, floor space. Enforced when booking.
    capacity         INTEGER  NOT NULL CHECK (capacity BETWEEN 1 AND 500),
    description      TEXT     CHECK (description IS NULL OR length(description) <= 300),

    -- Archived, never deleted: bookings reference this row, and "who was in
    -- Tuesday's HIIT" has to stay answerable after the gym drops the class
    -- from its timetable.
    archived_at      TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One class of a given name may not start twice in the same slot. Two
-- DIFFERENT classes at the same time are fine and normal (studio + main floor).
CREATE UNIQUE INDEX gym_classes_unique_slot
    ON gym_classes (gym_id, name, weekday, starts_at)
    WHERE archived_at IS NULL;

-- The timetable read: every live class for a gym, in the order it is shown.
CREATE INDEX gym_classes_timetable
    ON gym_classes (gym_id, weekday, starts_at)
    WHERE archived_at IS NULL;

-- "Your classes" for one instructor.
CREATE INDEX gym_classes_instructor ON gym_classes (gym_id, instructor_id)
    WHERE archived_at IS NULL;

COMMENT ON TABLE gym_classes IS
    'A recurring weekly class slot. One OCCURRENCE is (this row, a date) and is '
    'derived at read time, never stored — see class_bookings.on_date.';

-- ------------------------------------------------------------- bookings

CREATE TABLE class_bookings (
    id          UUID NOT NULL,
    gym_id      UUID NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    class_id    UUID NOT NULL REFERENCES gym_classes(id),
    member_id   UUID NOT NULL REFERENCES users(id),

    -- WHICH occurrence. Without this a booking means "Zumba, forever", and a
    -- member who came once holds a place every Monday until someone notices.
    on_date     DATE NOT NULL,

    -- Cancelling keeps the row. A deleted booking cannot answer "she booked
    -- and dropped out twice this month", and the partial unique index below is
    -- what lets the same member re-book the same occurrence afterwards.
    cancelled_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Client-minted ids (ADR-0008), so a retried booking replays the same id
    -- and lands as a no-op rather than a second place.
    PRIMARY KEY (id)
);

-- One live place per member per occurrence. Partial, so a cancelled booking
-- does not block re-booking — which is the whole reason cancellation is a
-- timestamp rather than a delete.
CREATE UNIQUE INDEX class_bookings_one_live_place
    ON class_bookings (class_id, on_date, member_id)
    WHERE cancelled_at IS NULL;

-- Counting a class's live bookings is the hot query: it runs for every row of
-- the timetable, to render "12/20" and to decide whether Book is offered.
CREATE INDEX class_bookings_occupancy
    ON class_bookings (gym_id, class_id, on_date)
    WHERE cancelled_at IS NULL;

-- "My bookings" for one member.
CREATE INDEX class_bookings_member
    ON class_bookings (gym_id, member_id, on_date)
    WHERE cancelled_at IS NULL;

COMMENT ON COLUMN class_bookings.on_date IS
    'The date of the occurrence booked. Must fall on the class''s weekday — '
    'enforced in the domain, where the gym''s timezone is known.';

-- ------------------------------------------------------------------ RLS

-- Tenant-owned, so both live under RLS like everything else carrying a gym_id
-- (ADR-0004). app_current_gym() rather than a hand-rolled current_setting:
-- the helper encodes the nullif(...,'') a committed GUC needs.
ALTER TABLE gym_classes ENABLE ROW LEVEL SECURITY;
ALTER TABLE gym_classes FORCE ROW LEVEL SECURITY;
CREATE POLICY gym_classes_tenant_isolation ON gym_classes
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE class_bookings ENABLE ROW LEVEL SECURITY;
ALTER TABLE class_bookings FORCE ROW LEVEL SECURITY;
CREATE POLICY class_bookings_tenant_isolation ON class_bookings
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

-- No DELETE on either. A class archives and a booking cancels; both are
-- referenced by something that has to stay readable, which is exactly the
-- distinction the operating-calendar tables did NOT have (a wrong opening hour
-- has no history hanging off it, so those may be deleted).
GRANT SELECT, INSERT, UPDATE ON gym_classes TO gym_app;
GRANT SELECT, INSERT, UPDATE ON class_bookings TO gym_app;
