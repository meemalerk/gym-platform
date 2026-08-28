#!/usr/bin/env bash
# Programme authoring, end to end over HTTP.
#
# The domain tests prove the lifecycle in isolation and
# verify-program-immutability.sh proves the database refuses to rewrite history.
# This proves the whole stack agrees: that a coach can actually author, review and
# publish a programme through the API, and that the refusals surface as 4xx with a
# usable message rather than a 500.
cd "$(dirname "$0")/.." || exit 1

# Scratch files live inside the repo, not /tmp. A bare `/tmp/x.json` is written
# by bash but READ BACK by python, and on Windows those are two different
# directories (Git Bash mounts /tmp at %LOCALAPPDATA%\Temp; a native python
# resolves the same string to C:	mp). Every jget/pyq silently returned
# nothing there, so the whole suite "passed" 0 assertions. A repo-relative
# path is the one spelling both agree on. Ignored by .gitignore via target/.
export VTMP="target/verify-tmp"
mkdir -p "$VTMP"

# `python3` is not a usable name everywhere: on Windows it resolves to the
# Microsoft Store stub, which prints nothing and exits 0 — so every jget
# returned empty and the suite reported passes it never ran. Resolve a real
# interpreter once, here, and call THAT.
if [ -z "${PY:-}" ]; then
  for candidate in python3 python py; do
    if command -v "$candidate" >/dev/null 2>&1        && "$candidate" -c 'import sys; sys.exit(0 if sys.version_info[0] == 3 else 1)' >/dev/null 2>&1; then
      PY="$candidate"; break
    fi
  done
fi
if [ -z "${PY:-}" ]; then echo "no python3 on PATH — cannot parse API responses" >&2; exit 1; fi
export PY
export DATABASE_URL="postgres://gym:gym_dev_password@localhost:5455/gym"
export APP_DATABASE_URL="${APP_DATABASE_URL:-postgres://gym_app:gym_app_dev_password@localhost:5455/gym}"
export SERVER_PORT=8096
export SERVER_HOST=127.0.0.1
export JWT_SECRET="dev-only-insecure-secret-change-me-before-any-real-deployment"
export RUST_LOG=warn
B="http://127.0.0.1:$SERVER_PORT"

# Reap leftover servers from earlier suites — and ONLY our own servers.
#
# Why this exists: on Windows a running server holds target/debug/server.exe
# open, so the next `cargo build` fails with "Access is denied", silently keeps
# the STALE binary, and that binary then refuses to start ("migration N was
# previously applied but is missing in the resolved migrations"). Every
# assertion reports 000 and the cause looks like a database problem. It is not.
# `trap ... EXIT` reaps a clean exit but not an interrupted one, so strays
# accumulate across runs — and a stray on ANY port holds the same executable,
# so clearing only $SERVER_PORT is not enough.
#
# Match on the PROCESS, never on the port. An earlier version of this swept
# ports 8089-8110 and killed whatever was listening; it took Docker Desktop
# down with it. A port number is not proof of identity — the image name is.
reap_stale_servers() {
  if command -v taskkill >/dev/null 2>&1; then
    # /IM matches the image name exactly, so nothing else can be caught.
    taskkill //F //IM server.exe >/dev/null 2>&1 || true
  elif command -v pkill >/dev/null 2>&1; then
    # -f plus the full path, so an unrelated binary called "server" survives.
    pkill -9 -f "$PWD/target/debug/server" 2>/dev/null || true
  fi
}
reap_stale_servers

cargo build --bin server 2>&1 | tail -2 || exit 1
./target/debug/server > /tmp/server-programs.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then PASS=$((PASS+1));
          else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi; }
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
pyq() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.load(open('$VTMP/b.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/b.json -w "%{http_code}" "$@"; }
uid() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/b.json; }

S=$(date +%s%N); PW="correct horse battery staple"
OWNER="prog-owner-$S@example.com"; COACH="prog-coach-$S@example.com"; MEMBER="prog-member-$S@example.com"

echo "=== setup: a gym, a head coach, a member, a catalogue ==="
OWNER_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$OWNER\",\"password\":\"$PW\",\"display_name\":\"Olive Owner\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Programme Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

# A second person, because publishing requires review by someone other than the author.
COACH_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$COACH\",\"password\":\"$PW\",\"display_name\":\"Casey Coach\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $COACH_T" >/dev/null
code -X PUT "$B/api/v1/gyms/$GYM/members/$(uid "$COACH_T")/capacities" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"capacities":["owner"]}' >/dev/null

MEMBER_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$MEMBER\",\"password\":\"$PW\",\"display_name\":\"Milo Member\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $MEMBER_T" >/dev/null
# Shut the door, now that BOTH people are in.
#
# It is opened once above and joined through twice, so this cannot sit next to
# the first join — closing it there locked the member out and turned every
# "member may not..." assertion into a 404 instead of the 403 it was testing.
#
# Left open, this gym joins the list a new member is offered at sign-up, which
# is how that list reached 163 rows.
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" \
  -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
  -d '{"open_registration":false}' >/dev/null

SQUAT=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Back Squat","modality":{"kind":"repetitions"}}' >/dev/null; jget "['id']" < $VTMP/b.json)
ROW=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Row 2k","modality":{"kind":"distance"}}' >/dev/null; jget "['id']" < $VTMP/b.json)

echo ""
echo "=== 1. authoring ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Beginner Strength","summary":"Three days a week."}')
check "create programme -> 201" "$ST" "201"
PROGRAM=$(pyq "d['id']"); V1=$(pyq "d['latest_version']['id']")
check "  starts at version 1" "$(pyq "d['latest_version']['version_number']")" "1"
check "  starts as a draft" "$(pyq "d['latest_version']['status']['state']")" "draft"
check "  a draft is editable" "$(pyq "d['latest_version']['is_editable']")" "True"
check "  a draft is NOT assignable" "$(pyq "d['latest_version']['is_assignable']")" "False"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Beginner Strength"}')
check "duplicate programme name -> 409" "$ST" "409"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":1,"label":"Accumulation"}')
check "add week -> 201" "$ST" "201"
WEEK=$(pyq "d['id']")

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":1}')
check "duplicate week -> 409" "$ST" "409"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":0}')
check "week zero -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$WEEK/workouts" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Upper A"}')
check "add workout -> 201" "$ST" "201"
WORKOUT=$(pyq "d['id']")

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$WORKOUT/exercises" \
  -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$SQUAT\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":4,\"target\":{\"min\":6,\"max\":8},\"rir\":2}}")
check "prescribe an exercise -> 201" "$ST" "201"
check "  position auto-assigned" "$(pyq "d['position']")" "1"

# The pairing check: a valid reps prescription against a distance exercise.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$WORKOUT/exercises" \
  -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$ROW\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":4,\"target\":{\"min\":6,\"max\":8}}}")
check "reps prescribed for a distance exercise -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$WORKOUT/exercises" \
  -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$SQUAT\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":99,\"target\":{\"min\":6,\"max\":8}}}")
check "99 sets -> 400" "$ST" "400"

echo ""
echo "=== 2. the review gate ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/transition" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '"publish"')
check "publish straight from draft -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/transition" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '"submit_for_review"')
check "submit for review -> 200" "$ST" "200"
check "  now in review" "$(pyq "d['status']['state']")" "in_review"
check "  no longer editable" "$(pyq "d['is_editable']")" "False"

# Frozen during review so a reviewer is not reading a moving target.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":2}')
check "edit while in review -> 400" "$ST" "400"

# The author is Olive; a staffed gym needs someone else to approve.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/transition" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '"approve"')
check "author approves own work -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/transition" -H "authorization: Bearer $COACH_T" \
  -H 'content-type: application/json' -d '"approve"')
check "head coach approves -> 200" "$ST" "200"
check "  now approved" "$(pyq "d['status']['state']")" "approved"
check "  approved is still NOT assignable" "$(pyq "d['is_assignable']")" "False"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/transition" -H "authorization: Bearer $COACH_T" \
  -H 'content-type: application/json' -d '"publish"')
check "publish -> 200" "$ST" "200"
check "  published" "$(pyq "d['status']['state']")" "published"
check "  now assignable" "$(pyq "d['is_assignable']")" "True"
check "  records who published it" "$(pyq "'published_by' in d['status']")" "True"

echo ""
echo "=== 3. a published version is immutable, through the API ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":3}')
check "add a week to a published version -> 400" "$ST" "400"
check "  and says why, in words a coach can act on" \
  "$(pyq "'new draft' in str(d).lower() or 'not editable' in str(d).lower() or 'no longer editable' in str(d).lower()")" "True"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$WEEK/workouts" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"day_number":2,"name":"Sneaky"}')
check "add a workout to a published version -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$WORKOUT/exercises" \
  -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$SQUAT\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":1,\"target\":{\"min\":1,\"max\":1}}}")
check "prescribe into a published version -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V1/transition" -H "authorization: Bearer $COACH_T" \
  -H 'content-type: application/json' -d '"return_to_draft"')
check "reopen a published version -> 400" "$ST" "400"

echo ""
echo "=== 4. editing a published programme creates a new draft ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs/$PROGRAM/versions" -H "authorization: Bearer $OWNER_T")
check "new draft from published -> 201" "$ST" "201"
V2=$(pyq "d['id']")
check "  is version 2" "$(pyq "d['version_number']")" "2"
check "  is a draft" "$(pyq "d['status']['state']")" "draft"
check "  records its lineage" "$(pyq "d['derived_from'] == '$V1'")" "True"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs/$PROGRAM/versions" -H "authorization: Bearer $OWNER_T")
check "a second open draft -> 409" "$ST" "409"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V2/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":1,"label":"Revised"}')
check "the new draft IS editable -> 201" "$ST" "201"

# The whole point: v1 is untouched by everything done to v2.
ST=$(code "$B/api/v1/gyms/$GYM/program-versions/$V1" -H "authorization: Bearer $OWNER_T")
check "published version still readable -> 200" "$ST" "200"
check "  still exactly one week" "$(pyq "len(d['weeks'])")" "1"
check "  still labelled 'Accumulation'" "$(pyq "d['weeks'][0]['label']")" "Accumulation"
check "  still one prescribed exercise" "$(pyq "len(d['exercises'])")" "1"
check "  prescription unchanged" "$(pyq "d['exercises'][0]['prescription']['sets']")" "4"
check "  exercise name resolved for display" "$(pyq "d['exercises'][0]['exercise_name']")" "Back Squat"

echo ""
echo "=== 5. who may do what ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $MEMBER_T" \
  -H 'content-type: application/json' -d '{"name":"Member Programme"}')
check "member creates a programme -> 403" "$ST" "403"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V2/weeks" -H "authorization: Bearer $MEMBER_T" \
  -H 'content-type: application/json' -d '{"week_number":5}')
check "member edits a draft -> 403" "$ST" "403"

# Members must be able to READ what they are being coached on.
ST=$(code "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $MEMBER_T")
check "member reads programmes -> 200" "$ST" "200"
check "  sees the programme" "$(pyq "any(p['name']=='Beginner Strength' for p in d)")" "True"

echo ""
echo "=== 6. tenant isolation ==="
OUT="prog-outsider-$S@example.com"
OUT_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$OUT\",\"password\":\"$PW\",\"display_name\":\"Ozzy Outsider\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
ST=$(code "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OUT_T")
check "outsider lists programmes -> 404 (not 403)" "$ST" "404"
ST=$(code "$B/api/v1/gyms/$GYM/program-versions/$V1" -H "authorization: Bearer $OUT_T")
check "outsider reads a version -> 404" "$ST" "404"

echo ""
echo "=== 7. the lifecycle is in the audit trail ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
check "audit readable -> 200" "$ST" "200"
for a in program.created program_version.submitted program_version.approved program_version.published; do
  check "  recorded $a" "$(pyq "any(e['action']=='$a' for e in d)")" "True"
done

echo ""
echo "=== 8. a gym with nobody to review (ADR-0031) ==="
# The second-person rule protects a gym from one person pushing unreviewed
# work to everybody. Where there is no second person it protects against
# nothing and prevents everything — the owner writes a programme, submits it,
# and the lifecycle stops dead with no legal move left. So it stands down when
# there is nobody else who could sign off, and comes back the moment there is.
SOLO_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \n  -d "{\"email\":\"pr-solo-$S@example.com\",\"password\":\"$PW\",\"display_name\":\"Sol Owner\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
SOLO=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Solo Box $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)
SOLO_EX=$(code -X POST "$B/api/v1/gyms/$SOLO/exercises" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"name":"Front Squat","modality":{"kind":"repetitions"}}' >/dev/null; jget "['id']" < $VTMP/b.json)

SOLO_P=$(code -X POST "$B/api/v1/gyms/$SOLO/programs" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"name":"Solo Block","focus":"strength"}' >/dev/null; jget "['latest_version']['id']" < $VTMP/b.json)
# A week alone is no longer reviewable content -- the gate counts PRESCRIBED
# EXERCISES, because a week (or a workout) with nothing in it used to publish
# fine and then open on a blank screen for the athlete. So prescribe something.
SOLO_W=$(code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P/weeks" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"week_number":1}' >/dev/null; jget "['id']" < $VTMP/b.json)
SOLO_WK=$(code -X POST "$B/api/v1/gyms/$SOLO/program-weeks/$SOLO_W/workouts" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Day 1"}' >/dev/null; jget "['id']" < $VTMP/b.json)
code -X POST "$B/api/v1/gyms/$SOLO/workout-templates/$SOLO_WK/exercises" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$SOLO_EX\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5},\"rir\":2}}" >/dev/null
code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P/transition" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null

ST=$(code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P/transition" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '"approve"')
check "the only person who can publish may approve their own work" "$ST" "200"
ST=$(code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P/transition" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '"publish"')
check "  and publish it, so the lifecycle finishes" "$ST" "200"

# Promote somebody who could review, and the rule comes back on its own.
REV_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \n  -d "{\"email\":\"pr-reviewer-$S@example.com\",\"password\":\"$PW\",\"display_name\":\"Ravi Reviewer\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
code -X PUT "$B/api/v1/gyms/$SOLO/settings/registration" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
code -X POST "$B/api/v1/gyms/$SOLO/join" -H "authorization: Bearer $REV_T" >/dev/null
# Shut the door again straight away.
#
# These gyms are throwaways, but the flag is not: onboarding lists every
# gym advertising an open door, so a suite that leaves one open puts a
# "Solo Box 1787848112219154700" in front of the next real person who
# signs up. Nineteen suites doing that is why that list had 163 rows.
# Capacities below do not need the door, so this is the moment to close it.
code -X PUT "$B/api/v1/gyms/$SOLO/settings/registration" \
  -H "authorization: Bearer $SOLO_T" -H 'content-type: application/json' \
  -d '{"open_registration":false}' >/dev/null
code -X PUT "$B/api/v1/gyms/$SOLO/members/$(uid "$REV_T")/capacities" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"capacities":["owner"]}' >/dev/null

SOLO_P2=$(code -X POST "$B/api/v1/gyms/$SOLO/programs" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"name":"Second Block","focus":"strength"}' >/dev/null; jget "['latest_version']['id']" < $VTMP/b.json)
SOLO_W2=$(code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P2/weeks" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"week_number":1}' >/dev/null; jget "['id']" < $VTMP/b.json)
SOLO_WK2=$(code -X POST "$B/api/v1/gyms/$SOLO/program-weeks/$SOLO_W2/workouts" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Day 1"}' >/dev/null; jget "['id']" < $VTMP/b.json)
code -X POST "$B/api/v1/gyms/$SOLO/workout-templates/$SOLO_WK2/exercises" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$SOLO_EX\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5},\"rir\":2}}" >/dev/null
code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P2/transition" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
ST=$(code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P2/transition" -H "authorization: Bearer $SOLO_T" \
  -H 'content-type: application/json' -d '"approve"')
check "once somebody else can review, self-approval is refused again" "$ST" "400"
ST=$(code -X POST "$B/api/v1/gyms/$SOLO/program-versions/$SOLO_P2/transition" -H "authorization: Bearer $REV_T" \
  -H 'content-type: application/json' -d '"approve"')
check "  and the second person may sign it off" "$ST" "200"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
