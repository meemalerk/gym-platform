#!/usr/bin/env bash
# Does the DATABASE refuse to rewrite a published programme?
#
# The domain layer already refuses (crates/domain/src/program.rs, and its tests).
# This checks the second line of defence: a bug, a maintenance script, or a future
# endpoint that forgets the rule must still be stopped. The invariant that a
# published version never changes is the one the whole product rests on — if it
# can be violated, every piece of training history built on it becomes suspect.
#
#   bash scripts/verify-program-immutability.sh
set -uo pipefail

# ON_ERROR_STOP is essential, not cosmetic: without it psql exits 0 even when the
# statement failed, so every "this must be refused" check silently passes and the
# script reports green while proving nothing.
PSQL=(docker exec -i gym-postgres psql -U gym -d gym -tAq -v ON_ERROR_STOP=1)

passed=0
failed=0

pass() { passed=$((passed + 1)); }
fail() { failed=$((failed + 1)); echo "  FAIL  $1"; }

# Runs SQL, expecting it to FAIL. The invariants here are all negative ones
# ("this must be refused"), so a silent success is the bug.
expect_rejected() {
  local label="$1" sql="$2"
  if echo "$sql" | "${PSQL[@]}" >/dev/null 2>&1; then
    fail "$label — was ALLOWED but must be refused"
  else
    pass
  fi
}

expect_ok() {
  local label="$1" sql="$2"
  local out
  if out=$(echo "$sql" | "${PSQL[@]}" 2>&1); then
    pass
  else
    fail "$label — $(echo "$out" | head -2 | tr '\n' ' ')"
  fi
}

echo "=== programme immutability (database level) ==="

# ------------------------------------------------------------------- fixture

ids=$(echo "
INSERT INTO gyms (id, name, slug, is_personal)
VALUES (gen_random_uuid(), 'Immutability Test Gym', 'immutability-test-' || substr(gen_random_uuid()::text, 1, 8), false)
RETURNING id;
" | "${PSQL[@]}")
GYM=$(echo "$ids" | head -1)

USER_ID=$(echo "
INSERT INTO users (id, email, display_name, password_hash)
VALUES (gen_random_uuid(), 'immutability-' || substr(gen_random_uuid()::text, 1, 8) || '@test.local', 'Test', 'x')
RETURNING id;
" | "${PSQL[@]}")

PROGRAM=$(echo "
INSERT INTO programs (id, gym_id, name, created_by)
VALUES (gen_random_uuid(), '$GYM', 'Test Programme', '$USER_ID')
RETURNING id;
" | "${PSQL[@]}")

VERSION=$(echo "
INSERT INTO program_versions (id, program_id, gym_id, version_number, status, created_by)
VALUES (gen_random_uuid(), '$PROGRAM', '$GYM', 1, 'draft', '$USER_ID')
RETURNING id;
" | "${PSQL[@]}")

if [ -z "$VERSION" ]; then
  echo "  FAIL  could not create fixture"
  exit 1
fi

# ------------------------------------------------------- a draft is editable

expect_ok "week can be added to a draft" "
INSERT INTO program_weeks (id, version_id, gym_id, week_number)
VALUES (gen_random_uuid(), '$VERSION', '$GYM', 1);"

WEEK=$(echo "SELECT id FROM program_weeks WHERE version_id = '$VERSION' LIMIT 1;" | "${PSQL[@]}")

expect_ok "workout can be added to a draft" "
INSERT INTO workout_templates (id, week_id, gym_id, day_number, name)
VALUES (gen_random_uuid(), '$WEEK', '$GYM', 1, 'Upper A');"

WORKOUT=$(echo "SELECT id FROM workout_templates WHERE week_id = '$WEEK' LIMIT 1;" | "${PSQL[@]}")
EXERCISE=$(echo "
INSERT INTO exercises (id, gym_id, name, modality)
VALUES (gen_random_uuid(), '$GYM', 'Back Squat', 'repetitions')
RETURNING id;" | "${PSQL[@]}")

expect_ok "exercise can be prescribed in a draft" "
INSERT INTO workout_template_exercises (id, workout_id, gym_id, exercise_id, position, prescription)
VALUES (gen_random_uuid(), '$WORKOUT', '$GYM', '$EXERCISE', 1,
        '{\"kind\":\"repetitions\",\"sets\":4,\"target\":{\"min\":6,\"max\":8},\"rir\":2}'::jsonb);"

# The status CHECK must reject a claim the application could not parse.
expect_rejected "prescription with an unknown kind" "
INSERT INTO workout_template_exercises (id, workout_id, gym_id, exercise_id, position, prescription)
VALUES (gen_random_uuid(), '$WORKOUT', '$GYM', '$EXERCISE', 2, '{\"kind\":\"telekinesis\"}'::jsonb);"

# ------------------------------------------------ evidence must match status

expect_rejected "published without a publisher" "
INSERT INTO program_versions (id, program_id, gym_id, version_number, status, created_by)
VALUES (gen_random_uuid(), '$PROGRAM', '$GYM', 99, 'published', '$USER_ID');"

# ------------------------------------------------------------ freeze on review

expect_ok "version moves to in_review" "
UPDATE program_versions
SET status = 'in_review', submitted_at = now(), submitted_by = '$USER_ID'
WHERE id = '$VERSION';"

# In review is frozen so a reviewer is not reading a moving target.
expect_rejected "week added while in review" "
INSERT INTO program_weeks (id, version_id, gym_id, week_number)
VALUES (gen_random_uuid(), '$VERSION', '$GYM', 2);"

expect_rejected "workout edited while in review" "
UPDATE workout_templates SET name = 'Sneaky Edit' WHERE id = '$WORKOUT';"

# ------------------------------------------------------------- publish, freeze

expect_ok "version is approved" "
UPDATE program_versions
SET status = 'approved', approved_at = now(), approved_by = '$USER_ID'
WHERE id = '$VERSION';"

expect_ok "version is published" "
UPDATE program_versions
SET status = 'published', published_at = now(), published_by = '$USER_ID'
WHERE id = '$VERSION';"

# The core invariant, from every angle a caller could try.
expect_rejected "add a week to a published version" "
INSERT INTO program_weeks (id, version_id, gym_id, week_number)
VALUES (gen_random_uuid(), '$VERSION', '$GYM', 3);"

expect_rejected "rename a published workout" "
UPDATE workout_templates SET name = 'Rewritten' WHERE id = '$WORKOUT';"

expect_rejected "change a published prescription" "
UPDATE workout_template_exercises
SET prescription = '{\"kind\":\"repetitions\",\"sets\":99,\"target\":{\"min\":1,\"max\":1}}'::jsonb
WHERE workout_id = '$WORKOUT';"

expect_rejected "delete an exercise from a published workout" "
DELETE FROM workout_template_exercises WHERE workout_id = '$WORKOUT';"

expect_rejected "delete a week from a published version" "
DELETE FROM program_weeks WHERE id = '$WEEK';"

expect_rejected "reopen a published version as a draft" "
UPDATE program_versions SET status = 'draft' WHERE id = '$VERSION';"

# A catalogue entry in use by a plan must not vanish underneath it.
expect_rejected "delete an exercise still referenced by a plan" "
DELETE FROM exercises WHERE id = '$EXERCISE';"

# -------------------------------------------------------- what IS still allowed

expect_ok "a published version can still be archived" "
UPDATE program_versions
SET status = 'archived', archived_at = now(), archived_by = '$USER_ID'
WHERE id = '$VERSION';"

expect_rejected "an archived version cannot be changed further" "
UPDATE program_versions SET status = 'draft' WHERE id = '$VERSION';"

# A new draft is a separate row, so it is editable even though v1 is frozen.
V2=$(echo "
INSERT INTO program_versions (id, program_id, gym_id, version_number, status, created_by, derived_from)
VALUES (gen_random_uuid(), '$PROGRAM', '$GYM', 2, 'draft', '$USER_ID', '$VERSION')
RETURNING id;" | "${PSQL[@]}")

expect_ok "a new draft version is editable beside the frozen one" "
INSERT INTO program_weeks (id, version_id, gym_id, week_number)
VALUES (gen_random_uuid(), '$V2', '$GYM', 1);"

# Two editable versions of one programme would let two coaches diverge silently.
expect_rejected "a second open draft for the same programme" "
INSERT INTO program_versions (id, program_id, gym_id, version_number, status, created_by)
VALUES (gen_random_uuid(), '$PROGRAM', '$GYM', 3, 'draft', '$USER_ID');"

# The frozen version must still be READABLE — assignments point at it.
remaining=$(echo "SELECT count(*) FROM program_weeks WHERE version_id = '$VERSION';" | "${PSQL[@]}")
if [ "$remaining" = "1" ]; then pass; else fail "published content survived intact (got $remaining weeks, want 1)"; fi

# ---------------------------------------------------------------------- cleanup

echo "DELETE FROM gyms WHERE id = '$GYM';" | "${PSQL[@]}" >/dev/null 2>&1
echo "DELETE FROM users WHERE id = '$USER_ID';" | "${PSQL[@]}" >/dev/null 2>&1

echo ""
echo "  PASSED: $passed    FAILED: $failed"
[ "$failed" -eq 0 ]
