-- Body measurements: person-owned wellness data, one row per person per day.
--
-- Same access model as profiles (outside RLS, service-gated — migration 0003
-- documents why person-owned tables sit outside the gym-keyed policies): you
-- write your own, your coaches and gym managers may read.
--
-- Height lives on the athlete PROFILE, not here: it is semi-static, and BMI is
-- computed at the edge from latest-weight + profile-height. Storing BMI would
-- be storing a derived number that drifts — same rule as progress metrics.
--
-- One row per (person, day), replaced on re-entry: a second weigh-in the same
-- morning is a correction, not a new fact. This also makes offline re-sync
-- naturally idempotent with no client id needed.

ALTER TABLE athlete_profiles
    ADD COLUMN height_cm INTEGER
        CHECK (height_cm IS NULL OR height_cm BETWEEN 50 AND 280);

CREATE TABLE body_measurements (
    user_id          UUID             NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    measured_on      DATE             NOT NULL,
    weight_kg        DOUBLE PRECISION CHECK (weight_kg IS NULL OR weight_kg BETWEEN 20 AND 500),
    body_fat_percent DOUBLE PRECISION CHECK (body_fat_percent IS NULL
                                             OR body_fat_percent BETWEEN 1 AND 75),
    waist_cm         DOUBLE PRECISION CHECK (waist_cm IS NULL OR waist_cm BETWEEN 10 AND 300),
    hip_cm           DOUBLE PRECISION CHECK (hip_cm IS NULL OR hip_cm BETWEEN 10 AND 300),
    chest_cm         DOUBLE PRECISION CHECK (chest_cm IS NULL OR chest_cm BETWEEN 10 AND 300),
    arm_cm           DOUBLE PRECISION CHECK (arm_cm IS NULL OR arm_cm BETWEEN 5 AND 100),
    thigh_cm         DOUBLE PRECISION CHECK (thigh_cm IS NULL OR thigh_cm BETWEEN 10 AND 150),
    notes            TEXT             CHECK (notes IS NULL OR length(notes) <= 500),
    created_at       TIMESTAMPTZ      NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ      NOT NULL DEFAULT now(),

    PRIMARY KEY (user_id, measured_on),

    -- A row with nothing in it is not a measurement.
    CONSTRAINT body_measurements_not_empty CHECK (
        weight_kg IS NOT NULL OR body_fat_percent IS NOT NULL
        OR waist_cm IS NOT NULL OR hip_cm IS NOT NULL OR chest_cm IS NOT NULL
        OR arm_cm IS NOT NULL OR thigh_cm IS NOT NULL
    )
);

CREATE INDEX body_measurements_recent_idx ON body_measurements (user_id, measured_on DESC);

-- Unlike training history, a person may DELETE their own measurement: this is
-- self-reported body data, not an accountability record — nobody else's work
-- hangs off it, and "I mistyped my weight last Tuesday" deserves an eraser.
GRANT SELECT, INSERT, UPDATE, DELETE ON body_measurements TO gym_app;
