#!/usr/bin/env bash
# Seeds a demo dataset for the single gym this deployment serves (ADR-0023):
# exactly three accounts — an owner, a trainer, and a member — each holding
# exactly one capacity. Nothing else.
#
# Everything goes through the real API, so the open door, capacity grants and the
# audit trail are all genuinely exercised — not inserted behind the app's back.
#
# Usage: bash scripts/seed-demo.sh [baseUrl]
set -uo pipefail

B="${1:-http://127.0.0.1:8080}"
PW="demopassword"

# Scratch file inside the repo, not /tmp — bash and a native Windows python
# resolve that string to different directories, so every jget silently
# returned nothing there. Same fix as the verify suites.
cd "$(dirname "$0")/.." || exit 1
VTMP="target/verify-tmp"
mkdir -p "$VTMP"

# `python3` is the Microsoft Store stub on Windows: prints nothing, exits 0.
if [ -z "${PY:-}" ]; then
  for candidate in python3 python py; do
    if command -v "$candidate" >/dev/null 2>&1        && "$candidate" -c 'import sys; sys.exit(0 if sys.version_info[0] == 3 else 1)' >/dev/null 2>&1; then
      PY="$candidate"; break
    fi
  done
fi
if [ -z "${PY:-}" ]; then echo "no python3 on PATH" >&2; exit 1; fi

jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
body() { curl -s -o $VTMP/seed.json -w "%{http_code}" "$@"; }

signup() { # signup <email> <name> -> token ("" if the email already exists)
  local st
  st=$(body -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
    -d "{\"email\":\"$1\",\"password\":\"$PW\",\"display_name\":\"$2\"}")
  if [ "$st" = "201" ]; then jget "['access_token']" < $VTMP/seed.json; else echo ""; fi
}

login() { # login <email> -> token
  body -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' \
    -d "{\"email\":\"$1\",\"password\":\"$PW\"}" >/dev/null
  jget "['access_token']" < $VTMP/seed.json
}

ensure() { # ensure <email> <name> -> token (signs up, or logs in if present)
  local t
  t=$(signup "$1" "$2")
  [ -z "$t" ] && t=$(login "$1")
  echo "$t"
}

mkgym() { # mkgym <token> <name> <is_personal> -> gym id
  # Idempotent: this is a single-gym deployment (ADR-0023) — `gyms` is capped
  # to one row by a database trigger when SINGLE_GYM_MODE is on (the demo sets
  # it) — so a re-run just finds the one gym already created rather than
  # getting a 409 back from a second attempt. `/me` still returns memberships
  # as a LIST at the wire level (the backend stays multi-gym-observable for
  # scripts/verify-rls.sh's sake); this account only ever has the one.
  local existing
  body "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null
  existing=$("$PY" -c "
import json
d = json.load(open('$VTMP/seed.json', encoding='utf-8'))
ms = d.get('memberships') or []
print(ms[0]['gym_id'] if ms else '')
" 2>/dev/null)

  if [ -n "$existing" ]; then
    echo "$existing"
    return
  fi

  body -X POST "$B/api/v1/gyms" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' \
    -d "{\"name\":\"$2\",\"is_personal\":$3}" >/dev/null
  jget "['id']" < $VTMP/seed.json
}

join() { # join <owner_token> <gym> <email> <their_token> <caps-json>
  # ADR-0031: invitations are gone. Everybody walks through the open door as a
  # member and the owner sets their standing afterwards — which is exactly what
  # the demo should be showing, because it is the only way a real gym does it
  # now. Idempotent: joining twice is a 409 we ignore, and setting the same
  # standing twice is a no-op.
  local uid st
  body -X PUT "$B/api/v1/gyms/$2/settings/registration" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
  body -X POST "$B/api/v1/gyms/$2/join" -H "authorization: Bearer $4" >/dev/null

  body "$B/api/v1/me" -H "authorization: Bearer $4" >/dev/null
  uid=$(jget "['user']['id']" < $VTMP/seed.json)

  st=$(body -X PUT "$B/api/v1/gyms/$2/members/$uid/capacities" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' -d "{\"capacities\":$5}")
  echo "    joined ($st): $3 -> $5"
}

exercise() { # exercise <token> <gym> <name> <modality> <notes>
  body -X POST "$B/api/v1/gyms/$2/exercises" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' \
    -d "{\"name\":\"$3\",\"modality\":{\"kind\":\"$4\"},\"notes\":\"$5\"}" >/dev/null
}

# Accounts are named for the capacity they demonstrate, not for invented people.
echo "=== accounts ==="
OWNER=$(ensure  "owner@demo.test"   "Owner")   ; echo "  owner"
TRAINER=$(ensure "trainer@demo.test" "Trainer") ; echo "  trainer"
# A SECOND trainer, and not padding: under ADR-0034 a trainer may prescribe only
# for their OWN clients, and a class roster is readable by that class's own
# instructor and nobody else. Neither rule is visible with one trainer, because
# there is no second party for them to be refused against.
TRAINER2=$(ensure "trainer2@demo.test" "Trainer 2") ; echo "  trainer 2"
MEMBER=$(ensure "member@demo.test"  "Member")  ; echo "  member"
# A member with NO coach, on the Open Gym plan — the case ADR-0035 exists for.
# Not padding either: `member@` has a coach and a programme, so with only that
# account there is nobody in the demo for whom "nobody will ever prescribe you
# anything" is true, and the build-your-own path has nothing to demonstrate on.
SOLO=$(ensure "solo@demo.test" "Sam Solo") ; echo "  solo"

echo ""
echo "=== the gym ==="
# ADR-0023: this deployment serves exactly one gym. The first POST /gyms call
# (this one) creates it and its owner; a database trigger 409s any further
# attempt, which is why every other account below walks through the open door
# and is promoted, instead of calling mkgym.
#
# is_personal=true: not a "solo workspace" in the literal sense (it has staff
# and members), but it is the flag that lets the SAME account both write and
# approve a programme version. With only three accounts and one of them (the
# owner) holding the only catalogue-management capacity, there is no second
# person to hand review to — exactly the case this flag exists for.
IRON=$(mkgym  "$OWNER" "Iron Box Strength" true); echo "  Iron Box Strength         $IRON"

echo ""
echo "=== staffing ==="
join "$OWNER" "$IRON" "trainer@demo.test" "$TRAINER" '["trainer"]'
join "$OWNER" "$IRON" "trainer2@demo.test" "$TRAINER2" '["trainer"]'
join "$OWNER" "$IRON" "member@demo.test"  "$MEMBER"  '["member"]'
join "$OWNER" "$IRON" "solo@demo.test"    "$SOLO"    '["member"]'

echo ""
echo "=== catalogue ==="
# Only the owner holds a catalogue-managing capacity here — see the
# is_personal note above.
exercise "$OWNER" "$IRON"   "Back Squat"        repetitions "Brace, then sit between the hips."
exercise "$OWNER" "$IRON"   "Deadlift"          repetitions "Push the floor away."
exercise "$OWNER" "$IRON"   "Plank"             duration    "Ribs down, glutes on."
exercise "$OWNER" "$IRON"   "Row"               distance    "Legs, hips, arms."
exercise "$OWNER" "$IRON"   "Bench Press"       repetitions "Shoulder blades pinned, feet planted."
exercise "$OWNER" "$IRON"   "Overhead Press"    repetitions "Glutes tight, ribs down, punch the ceiling."
exercise "$OWNER" "$IRON"   "Barbell Row"       repetitions "Hinge, pull to the sternum."
exercise "$OWNER" "$IRON"   "Front Squat"       repetitions "Elbows high, sit tall."
exercise "$OWNER" "$IRON"   "Romanian Deadlift" repetitions "Hips back until the hamstrings argue."
exercise "$OWNER" "$IRON"   "Pull-Up"           repetitions "From a dead hang, chin over."
exercise "$OWNER" "$IRON"   "Farmer Carry"      distance    "Heavy hands, proud chest."
exercise "$OWNER" "$IRON"   "Bike Erg"          distance    "Smooth cadence, nose breathing."
exercise "$OWNER" "$IRON"   "Turkish Get-Up"    repetitions "Slow. Eyes on the bell."
echo "  seeded"

pys() { "$PY" -c "import json,sys;d=json.load(open('$VTMP/seed.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }

# Look an Iron Box exercise up by name — the prescriptions below need ids.
exid() {
  body "$B/api/v1/gyms/$IRON/exercises" -H "authorization: Bearer $OWNER" >/dev/null
  "$PY" -c "import json,sys;d=json.load(open('$VTMP/seed.json', encoding='utf-8'));print(next((e['id'] for e in d if e['name']==sys.argv[1]),''))" "$1"
}

echo ""
echo "=== coaching relationship ==="
# Assigned by the owner, who holds the catalogue-managing capacity that pairing
# requires — a trainer pairing themselves with a client would be a self-service
# grant of access to that person's data.
uid() { body "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/seed.json; }

TRAINER_ID=$(uid "$TRAINER"); MEMBER_ID=$(uid "$MEMBER"); SOLO_ID=$(uid "$SOLO")

# solo@ is deliberately NOT paired with anybody. That absence is the point.

# The owner PROPOSES a coach and the trainer ACCEPTS (ADR-0034). Two calls,
# because a coaching relationship hands a trainer somebody else's whole
# training history and they get asked first. There is no direct-pairing
# endpoint any more, so this is the only route and the seed shows it.
pair() { # pair <proposer_token> <coach_token> <gym> <coach_id> <athlete_id> <label>
  local rid st
  rid=$(body -X POST "$B/api/v1/gyms/$3/coaching-requests/propose" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' \
    -d "{\"athlete_id\":\"$5\",\"coach_id\":\"$4\"}" >/dev/null; jget "['id']" < $VTMP/seed.json)
  if [ -z "$rid" ]; then echo "    $6 (already paired)"; return 0; fi
  st=$(body -X POST "$B/api/v1/gyms/$3/coaching-requests/$rid/answer" -H "authorization: Bearer $2" \
    -H 'content-type: application/json' -d '"accept"')
  echo "    $6 — proposed by the owner, accepted by the trainer ($st)"
}

pair "$OWNER" "$TRAINER" "$IRON" "$TRAINER_ID" "$MEMBER_ID" "trainer@ coaches member@"

echo ""
echo "=== a published programme ==="
# Walked all the way to Published so the immutability rule is visible in the
# demo data: this version can no longer be edited, only superseded.
prog() {
  body -X POST "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d '{"name":"Beginner Strength","summary":"Three days a week, eight weeks.","focus":"strength"}' >/dev/null
  jget "['latest_version']['id']" < $VTMP/seed.json
}
V=$(prog)

if [ -n "$V" ]; then
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$V/weeks" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '{"week_number":1,"label":"Accumulation"}' >/dev/null
  WEEK=$(jget "['id']" < $VTMP/seed.json)

  body -X POST "$B/api/v1/gyms/$IRON/program-weeks/$WEEK/workouts" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '{"day_number":1,"name":"Lower A"}' >/dev/null
  WK=$(jget "['id']" < $VTMP/seed.json)

  body "$B/api/v1/gyms/$IRON/exercises" -H "authorization: Bearer $OWNER" >/dev/null
  SQUAT=$("$PY" -c "import json;print(next(e['id'] for e in json.load(open('$VTMP/seed.json', encoding='utf-8')) if e['name']=='Back Squat'))" 2>/dev/null)

  # Fail loudly. This used to be `[ -n "$SQUAT" ] && ...`, so a failed id lookup
  # silently skipped the only exercise and the script published the version
  # anyway -- and a published version is frozen, so the demo shipped a workout
  # that opened on a blank screen and could never be repaired. Never publish
  # content we did not verify we wrote.
  if [ -z "$SQUAT" ]; then
    echo "    FATAL: could not resolve the 'Back Squat' exercise id." >&2
    echo "    Refusing to publish Beginner Strength v1 with an empty workout." >&2
    exit 1
  fi
  body -X POST "$B/api/v1/gyms/$IRON/workout-templates/$WK/exercises" \
    -H "authorization: Bearer $OWNER" -H 'content-type: application/json' \
    -d "{\"exercise_id\":\"$SQUAT\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":4,\"target\":{\"min\":6,\"max\":8},\"rir\":2}}" >/dev/null

  # Draft -> in review -> approved -> published, all by the owner: the personal-
  # gym exception (is_personal=true above) is what allows one person to both
  # write and approve — see the note by the mkgym call.
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$V/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$V/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"approve"' >/dev/null
  st=$(body -X POST "$B/api/v1/gyms/$IRON/program-versions/$V/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"publish"')
  echo "    Beginner Strength v1 -> $(jget "['status']['state']" < $VTMP/seed.json 2>/dev/null || echo "$st")"
else
  echo "    (programme already exists)"
fi

echo ""
echo "=== a second programme, richer, published ==="
# Reused builders. Each swallows conflicts so re-runs stay quiet.
addweek() { # addweek <version> <n> <label> -> week id
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$1/weeks" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d "{\"week_number\":$2,\"label\":\"$3\"}" >/dev/null
  jget "['id']" < $VTMP/seed.json
}
addworkout() { # addworkout <week> <day> <name> -> workout id
  body -X POST "$B/api/v1/gyms/$IRON/program-weeks/$1/workouts" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d "{\"day_number\":$2,\"name\":\"$3\"}" >/dev/null
  jget "['id']" < $VTMP/seed.json
}
rx_reps() { # rx_reps <workout> <exercise_name> <sets> <min> <max> <rir>
  body -X POST "$B/api/v1/gyms/$IRON/workout-templates/$1/exercises" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"exercise_id\":\"$(exid "$2")\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":$3,\"target\":{\"min\":$4,\"max\":$5},\"rir\":$6}}" >/dev/null
}
rx_time() { # rx_time <workout> <exercise_name> <sets> <seconds>
  body -X POST "$B/api/v1/gyms/$IRON/workout-templates/$1/exercises" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"exercise_id\":\"$(exid "$2")\",\"prescription\":{\"kind\":\"duration\",\"sets\":$3,\"seconds\":$4}}" >/dev/null
}
rx_dist() { # rx_dist <workout> <exercise_name> <metres>
  body -X POST "$B/api/v1/gyms/$IRON/workout-templates/$1/exercises" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"exercise_id\":\"$(exid "$2")\",\"prescription\":{\"kind\":\"distance\",\"metres\":$3}}" >/dev/null
}

body "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" >/dev/null
HYP_EXISTS=$(pys "any(p['name']=='Hypertrophy Block' for p in d)")

if [ "$HYP_EXISTS" != "True" ]; then
  body -X POST "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d '{"name":"Hypertrophy Block","summary":"Six days over two weeks. Chase the pump, keep two in the tank.","focus":"hypertrophy"}' >/dev/null
  HV=$(pys "d['latest_version']['id']")

  for weeknum in 1 2; do
    label="Volume"; [ "$weeknum" = "2" ] && label="Volume, heavier"
    WK=$(addweek "$HV" "$weeknum" "$label")
    PUSH=$(addworkout "$WK" 1 "Push")
    rx_reps "$PUSH" "Bench Press" 4 8 10 2
    rx_reps "$PUSH" "Overhead Press" 3 10 12 2
    rx_time "$PUSH" "Plank" 3 60
    PULL=$(addworkout "$WK" 3 "Pull")
    rx_reps "$PULL" "Barbell Row" 4 8 10 2
    rx_reps "$PULL" "Pull-Up" 3 6 10 1
    rx_dist "$PULL" "Farmer Carry" 120
    LEGS=$(addworkout "$WK" 5 "Legs")
    rx_reps "$LEGS" "Front Squat" 4 6 8 2
    rx_reps "$LEGS" "Romanian Deadlift" 3 8 10 2
    rx_dist "$LEGS" "Bike Erg" 2000
  done

  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$HV/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$HV/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"approve"' >/dev/null
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$HV/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"publish"' >/dev/null
  echo "    Hypertrophy Block: 2 weeks x 3 workouts -> $(pys "d['status']['state']")"
else
  echo "    (Hypertrophy Block already exists)"
fi

echo ""
echo "=== a programme still in review — the lifecycle, visible ==="
body "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" >/dev/null
CON_EXISTS=$(pys "any(p['name']=='Conditioning Base' for p in d)")

if [ "$CON_EXISTS" != "True" ]; then
  body -X POST "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d '{"name":"Conditioning Base","summary":"Aerobic base for the off-season.","focus":"conditioning"}' >/dev/null
  CV=$(pys "d['latest_version']['id']")
  CWK=$(addweek "$CV" 1 "Base")
  CW=$(addworkout "$CWK" 2 "Intervals")
  rx_dist "$CW" "Bike Erg" 2000
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$CV/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
  echo "    Conditioning Base -> $(pys "d['status']['state']") (awaiting approval)"
else
  echo "    (Conditioning Base already exists)"
fi

echo ""
echo "=== the door is left open, so a viewer can join and look around ==="
# ADR-0031 removed invitations, and the seed used to leave one pending here so
# the People screen had something to show. The open door replaces it, and it
# makes a better demo: somebody reading START-HERE can sign up and walk in,
# rather than reading about a link they will never receive.
body -X PUT "$B/api/v1/gyms/$IRON/settings/registration" -H "authorization: Bearer $OWNER" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
echo "    open registration on — anyone can join as a member"

echo ""
echo "=== assignments ==="
# Look the published version up from the list rather than trusting $V - on a
# re-run the programme already exists and $V is empty.
#
# `latest_version` is not enough. Further down, this same script gives Beginner
# Strength a v2 DRAFT on purpose (the lifecycle, visible), so from the second
# run onwards its latest version is that draft: not assignable, and the whole
# section printed "(no published version found)" and quietly skipped both the
# assignments and the three weeks of history that depend on them. The data
# survived only because it had already been seeded on run one.
#
# So: take the latest version if it is assignable, and otherwise ask that
# programme for its versions and take the newest one that is.
pub_version_of() { # pub_version_of <programme name>
  local prog_id ver
  body "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" >/dev/null
  ver=$("$PY" -c "
import json,sys
d = json.load(open('$VTMP/seed.json', encoding='utf-8'))
print(next((p['latest_version']['id'] for p in d
            if p['name'] == sys.argv[1] and p['latest_version']['is_assignable']), ''))
" "$1" 2>/dev/null)
  if [ -n "$ver" ]; then echo "$ver"; return; fi

  prog_id=$("$PY" -c "
import json,sys
d = json.load(open('$VTMP/seed.json', encoding='utf-8'))
print(next((p['id'] for p in d if p['name'] == sys.argv[1]), ''))
" "$1" 2>/dev/null)
  [ -z "$prog_id" ] && { echo ""; return; }

  body "$B/api/v1/gyms/$IRON/programs/$prog_id/versions" -H "authorization: Bearer $OWNER" >/dev/null
  "$PY" -c "
import json
d = json.load(open('$VTMP/seed.json', encoding='utf-8'))
ok = [v for v in d if v.get('is_assignable')]
ok.sort(key=lambda v: v.get('version_number', 0), reverse=True)
print(ok[0]['id'] if ok else '')
" 2>/dev/null
}

PUB_V=$(pub_version_of 'Beginner Strength')

if [ -n "$PUB_V" ]; then
  # Backdated three weeks so the session history below is coherent with it.
  START=$(date -d "-21 days" +%F)
  # Assigned by the TRAINER, not the owner (ADR-0034). Picking the right
  # programme for the person in front of you is coaching; the owner writes the
  # catalogue and decides who coaches whom, and does not prescribe past them.
  assign() { # assign <athlete_id> <version> <start> <label>
    local st
    st=$(body -X POST "$B/api/v1/gyms/$IRON/program-assignments" -H "authorization: Bearer $TRAINER" \
      -H 'content-type: application/json' \
      -d "{\"athlete_id\":\"$1\",\"program_version_id\":\"$2\",\"start_date\":\"$3\"}")
    echo "    $4 ($st)"
  }
  assign "$MEMBER_ID" "$PUB_V" "$START" "member@ starts Beginner Strength"

  # member@ also runs the hypertrophy block — two concurrent programmes is the
  # normal case for a serious member, and the Today screen should show both.
  HYP_V=$(pub_version_of 'Hypertrophy Block')
  [ -n "$HYP_V" ] && assign "$MEMBER_ID" "$HYP_V" "$(date -d '-7 days' +%F)" "member@ also starts Hypertrophy Block"
else
  echo "    (no published version found)"
fi

echo ""
echo "=== three weeks of training history ==="
# Deterministic ids (uuid5 of a seed tag) + the server's idempotent inserts mean
# this whole block is safe to re-run: every replay is a no-op, not a duplicate.
suid() { "$PY" -c "import uuid,sys;print(uuid.uuid5(uuid.NAMESPACE_URL, sys.argv[1]))" "$1"; }
ago() { date -u -d "-$1 days" "+%Y-%m-%dT10:30:00Z"; }

my_assignment() { # my_assignment <token> <program_name> -> assignment id
  body "$B/api/v1/gyms/$IRON/program-assignments" -H "authorization: Bearer $1" >/dev/null
  "$PY" -c "
import json,sys
d = json.load(open('$VTMP/seed.json', encoding='utf-8'))
print(next((a['id'] for a in d
            if a['program_name'] == sys.argv[1] and a['is_active']
            and a['athlete_id'] == sys.argv[2]), ''))
" "$2" "$3"
}

# The Lower A workout and its squat prescription, from the published version.
body "$B/api/v1/gyms/$IRON/program-versions/$PUB_V" -H "authorization: Bearer $OWNER" >/dev/null
LOWER_A=$(pys "d['workouts'][0]['id']")
SQUAT_TX=$(pys "d['exercises'][0]['id']")
SQUAT_EX=$(pys "d['exercises'][0]['exercise_id']")

log_history() { # log_history <token> <athlete_id> <tag> <days_ago> <weight> <rpe> <outcome> <sets>
  local token="$1" athlete="$2" tag="$3" days="$4" weight="$5" rpe="$6" outcome="$7" sets="$8"
  local aid sid n
  aid=$(my_assignment "$token" "Beginner Strength" "$athlete")
  [ -z "$aid" ] && { echo "    (no active Beginner Strength assignment for $tag)"; return; }
  sid=$(suid "seed-session-$tag")
  body -X POST "$B/api/v1/gyms/$IRON/workout-sessions" -H "authorization: Bearer $token" \
    -H 'content-type: application/json' \
    -d "{\"id\":\"$sid\",\"assignment_id\":\"$aid\",\"workout_template_id\":\"$LOWER_A\",\"started_at\":\"$(ago "$days")\"}" >/dev/null
  for n in $(seq 1 "$sets"); do
    body -X POST "$B/api/v1/gyms/$IRON/workout-sessions/$sid/sets" -H "authorization: Bearer $token" \
      -H 'content-type: application/json' \
      -d "{\"id\":\"$(suid "seed-set-$tag-$n")\",\"exercise_id\":\"$SQUAT_EX\",\"template_exercise_id\":\"$SQUAT_TX\",\"set_number\":$n,\"performed\":{\"kind\":\"repetitions\",\"reps\":5,\"weight_kg\":$weight},\"rpe\":$rpe}" >/dev/null
  done
  if [ "$outcome" != "open" ]; then
    body -X POST "$B/api/v1/gyms/$IRON/workout-sessions/$sid/finish" -H "authorization: Bearer $token" \
      -H 'content-type: application/json' -d "\"$outcome\"" >/dev/null
  fi
}

if [ -n "$LOWER_A" ] && [ -n "$SQUAT_EX" ]; then
  # member@: a believable progression, one honest failure, one workout cut
  # short, and a session still open right now.
  log_history "$MEMBER" "$MEMBER_ID" "m-1" 21 60   7  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-2" 18 62.5 7  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-3" 16 62.5 8  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-4" 14 65   8  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-5" 11 67.5 8  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-6" 9  67.5 9  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-7" 7  70   9  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-8" 4  72.5 9  completed 3
  log_history "$MEMBER" "$MEMBER_ID" "m-9" 2  72.5 10 abandoned 1
  log_history "$MEMBER" "$MEMBER_ID" "m-10" 0 70   8  open      2
  echo "    member@: 8 completed, 1 abandoned, 1 in progress (squat 60 -> 72.5 kg)"
else
  echo "    (could not resolve Lower A / Back Squat from the published version)"
fi

echo ""
echo "=== body data: height + three weeks of weigh-ins ==="
# Height on the profile (feeds BMI); weight as a gently falling series. PUT by
# date is idempotent, so re-runs replace rather than duplicate.
body -X PUT "$B/api/v1/me/profiles/athlete" -H "authorization: Bearer $MEMBER" \
  -H 'content-type: application/json' \
  -d '{"goals":"Squat 100 kg and feel human again.","training_age_months":18,"limitations":"Left knee: no deep lunges.","height_cm":178}' >/dev/null

for spec in "20:82.0" "17:81.6" "14:81.1" "11:80.9" "8:80.4" "5:80.2" "2:79.8" "0:79.6"; do
  days="${spec%%:*}"; kg="${spec##*:}"
  body -X PUT "$B/api/v1/me/measurements/$(date -d "-$days days" +%F)" \
    -H "authorization: Bearer $MEMBER" -H 'content-type: application/json' \
    -d "{\"weight_kg\":$kg}" >/dev/null
done
# One fuller entry with tape measurements.
body -X PUT "$B/api/v1/me/measurements/$(date -d "-2 days" +%F)" \
  -H "authorization: Bearer $MEMBER" -H 'content-type: application/json' \
  -d '{"weight_kg":79.8,"body_fat_percent":19.2,"waist_cm":87.0}' >/dev/null
echo "    member@: 178 cm, 82.0 -> 79.6 kg over three weeks"

echo ""
echo "=== trainer profiles & a conditioning programme to recommend ==="
# owner@ speaks strength and does NOT coach member@ — so member@'s lift goal
# can suggest them. trainer@ already coaches member@; excluded by the rule.
body -X PUT "$B/api/v1/me/profiles/trainer" -H "authorization: Bearer $OWNER" \
  -H 'content-type: application/json' \
  -d '{"headline":"Owner-operator — strength first","specialties":["Powerlifting","Programming"]}' >/dev/null
body -X PUT "$B/api/v1/me/profiles/trainer" -H "authorization: Bearer $TRAINER" \
  -H 'content-type: application/json' \
  -d '{"headline":"Coach for beginners","specialties":["Beginners","Conditioning"]}' >/dev/null

# Published, conditioning-focused, assigned to nobody: member@ has a cut goal,
# so this is what the engine should surface.
body "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" >/dev/null
ENGINE_EXISTS=$(pys "any(p['name']=='Engine Builder' for p in d)")
if [ "$ENGINE_EXISTS" != "True" ]; then
  body -X POST "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d '{"name":"Engine Builder","summary":"Zone 2 base with weekly intervals.","focus":"conditioning"}' >/dev/null
  EV=$(pys "d['latest_version']['id']")
  EWK=$(addweek "$EV" 1 "Base")
  EW=$(addworkout "$EWK" 2 "Long Ride")
  rx_dist "$EW" "Bike Erg" 8000
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$EV/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$EV/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"approve"' >/dev/null
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$EV/transition" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '"publish"' >/dev/null
  echo "    Engine Builder (conditioning) -> $(pys "d['status']['state']")"
else
  echo "    (Engine Builder already exists)"
fi

echo ""
echo "=== goals — measurable, with baselines ==="
body "$B/api/v1/gyms/$IRON/goals" -H "authorization: Bearer $MEMBER" >/dev/null
HAVE_GOALS=$(pys "len(d) > 0")

if [ "$HAVE_GOALS" != "True" ]; then
  DEADLINE=$(date -d "+70 days" +%F)
  # member@ sets their own cut — self-service is the point of goals.
  body -X POST "$B/api/v1/gyms/$IRON/goals" -H "authorization: Bearer $MEMBER" \
    -H 'content-type: application/json' \
    -d "{\"athlete_id\":\"$MEMBER_ID\",\"metric\":{\"kind\":\"bodyweight\",\"baseline_kg\":82.0,\"target_kg\":78.0},\"target_date\":\"$DEADLINE\"}" >/dev/null
  # Their coach (trainer@) sets the squat target — allowed because they hold
  # the active coaching relationship, not because of a special capacity.
  SQUAT_ID=$(exid "Back Squat")
  body -X POST "$B/api/v1/gyms/$IRON/goals" -H "authorization: Bearer $TRAINER" \
    -H 'content-type: application/json' \
    -d "{\"athlete_id\":\"$MEMBER_ID\",\"metric\":{\"kind\":\"exercise_est_1rm\",\"exercise_id\":\"$SQUAT_ID\",\"baseline_kg\":70.0,\"target_kg\":100.0}}" >/dev/null
  echo "    member@: cut to 78 kg (own) + squat est 1RM to 100 kg (coach-set)"
else
  echo "    (goals already seeded)"
fi

echo ""
echo "=== billing — plans, subscriptions, and money in every state ==="
body "$B/api/v1/gyms/$IRON/plans" -H "authorization: Bearer $OWNER" >/dev/null
HAVE_PLANS=$(pys "len(d) > 0")

if [ "$HAVE_PLANS" != "True" ]; then
  mkplan() {  # name price_minor interval description grants-json
    body -X POST "$B/api/v1/gyms/$IRON/plans" -H "authorization: Bearer $OWNER" \
      -H 'content-type: application/json' \
      -d "{\"name\":\"$1\",\"price_minor\":$2,\"currency\":\"GBP\",\"interval\":\"$3\",\"description\":\"$4\",\"grants\":$5}" >/dev/null
    pys "d['id']"
  }
  # Grants are what the plan actually confers — the tier ladder is visible in
  # this list, not in the price. Coaching is Open Gym plus the coached feature.
  OPEN_GYM=$(mkplan "Open Gym" 4900 monthly "Floor access, open hours" '["gym_access"]')
  COACHING=$(mkplan "Coaching" 12000 monthly "Open Gym plus an assigned coach and programme" \
    '["gym_access","coached_programming"]')
  mkplan "Drop-in" 1500 once "A single visit, no commitment" '["gym_access"]' >/dev/null

  TODAY_D=$(date +%F)
  LAST_MONTH=$(date -d "-1 month" +%F)

  # member@ on coaching, paid in full.
  body -X POST "$B/api/v1/gyms/$IRON/subscriptions" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"member_id\":\"$MEMBER_ID\",\"plan_id\":\"$COACHING\",\"started_on\":\"$LAST_MONTH\"}" >/dev/null
  PAID_INV=$(pys "d['id']")
  body -X POST "$B/api/v1/gyms/$IRON/invoices/$PAID_INV/payments" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"amount_minor\":12000,\"provider\":\"card_terminal\",\"received_on\":\"$LAST_MONTH\"}" >/dev/null

  # solo@ on Open Gym: gym access, no coached programming. This is the
  # membership most gyms sell most of, and until ADR-0035 the app recorded
  # nothing at all for it.
  body -X POST "$B/api/v1/gyms/$IRON/subscriptions" -H "authorization: Bearer $OWNER"     -H 'content-type: application/json'     -d "{\"member_id\":\"$SOLO_ID\",\"plan_id\":\"$OPEN_GYM\",\"started_on\":\"$LAST_MONTH\"}" >/dev/null
  SOLO_INV=$(pys "d['id']")
  body -X POST "$B/api/v1/gyms/$IRON/invoices/$SOLO_INV/payments" -H "authorization: Bearer $OWNER"     -H 'content-type: application/json'     -d "{\"amount_minor\":4900,\"provider\":\"card_terminal\",\"received_on\":\"$LAST_MONTH\"}" >/dev/null

  # A part-paid drop-in on the same member, so the screen shows an invoice
  # that is neither settled nor untouched — the state a gym actually argues
  # about — without needing a second billable person.
  body -X POST "$B/api/v1/gyms/$IRON/invoices" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"member_id\":\"$MEMBER_ID\",\"description\":\"Drop-in · guest\",\"amount_minor\":1500,\"currency\":\"GBP\",\"due_on\":\"$TODAY_D\"}" >/dev/null
  PART_INV=$(pys "d['id']")
  body -X POST "$B/api/v1/gyms/$IRON/invoices/$PART_INV/payments" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"amount_minor\":500,\"provider\":\"cash\",\"received_on\":\"$TODAY_D\",\"note\":\"paid what they had\"}" >/dev/null

  echo "    Open Gym £49/mo · Coaching £120/mo · Drop-in £15"
  echo "    member@: coaching paid; one part-paid drop-in"
  echo "    solo@:   Open Gym paid, no coach — builds their own workouts"
else
  echo "    (billing already seeded)"
fi

echo ""
echo "=== the states a happy path never produces ==="
# Everything above is a gym going well. These are the rules the product is
# actually built around, and none of them are visible unless something has been
# archived, refused or undone. A demo that only shows the happy path
# demonstrates a to-do list.

# --- a draft, so the authoring UI has something to author -------------------
body "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" >/dev/null
HAVE_DRAFT=$(pys "any(p['name'] == 'Deload Week' for p in d)")

if [ "$HAVE_DRAFT" != "True" ]; then
  body -X POST "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d '{"name":"Deload Week","summary":"Half the volume, none of the heroics.","focus":"general"}' >/dev/null
  DRAFT_V=$(pys "d['latest_version']['id']")
  body -X POST "$B/api/v1/gyms/$IRON/program-versions/$DRAFT_V/weeks" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '{"week_number":1,"label":"Deload"}' >/dev/null
  DRAFT_W=$(pys "d['id']")
  body -X POST "$B/api/v1/gyms/$IRON/program-weeks/$DRAFT_W/workouts" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d '{"day_number":1,"name":"Full body, light","notes":"Everything at half the usual load."}' >/dev/null
  echo "    Deload Week — left as a DRAFT, so Add week / Add workout / Publish are live"
else
  echo "    (draft already seeded)"
fi

# --- v2 of a published programme: the immutability rule, made visible -------
# This is the single most important thing the demo can show. v1 stays published
# and assigned; v2 is a separate draft. Editing did not touch what anyone is on.
body "$B/api/v1/gyms/$IRON/programs" -H "authorization: Bearer $OWNER" >/dev/null
BEGINNER_P=$(pys "next((p['id'] for p in d if p['name'] == 'Beginner Strength'), '')")
if [ -n "$BEGINNER_P" ]; then
  body "$B/api/v1/gyms/$IRON/programs/$BEGINNER_P/versions" -H "authorization: Bearer $OWNER" >/dev/null
  VERSION_COUNT=$(pys "len(d)")
  if [ "$VERSION_COUNT" = "1" ]; then
    body -X POST "$B/api/v1/gyms/$IRON/programs/$BEGINNER_P/versions" \
      -H "authorization: Bearer $OWNER" -H 'content-type: application/json' >/dev/null
    echo "    Beginner Strength now has v1 (published, people on it) + v2 (draft)"
  else
    echo "    (second version already seeded)"
  fi
fi

# --- an achieved goal, so progress has an ending ----------------------------
body "$B/api/v1/gyms/$IRON/goals" -H "authorization: Bearer $MEMBER" >/dev/null
CLOSED=$(pys "sum(1 for g in d if not g['is_active'])")
OPEN_GOAL=$(pys "next((g['id'] for g in d if g['is_active'] and g['athlete_id'] == '$MEMBER_ID'), '')")
if [ "$CLOSED" = "0" ] && [ -n "$OPEN_GOAL" ]; then
  # Close the FIRST of member@'s goals; the other stays open and tracking.
  body -X POST "$B/api/v1/gyms/$IRON/goals/$OPEN_GOAL/close" -H "authorization: Bearer $MEMBER" \
    -H 'content-type: application/json' -d '"achieved"' >/dev/null
  echo "    one of member@'s goals closed as achieved"
else
  echo "    (a closed goal already exists)"
fi

# --- a voided invoice and a refund ------------------------------------------
# An issued invoice is immutable, so a correction is a VOID plus a new one, and
# a refund is a negative payment. Neither is an edit. Both need to be seen.
body "$B/api/v1/gyms/$IRON/invoices" -H "authorization: Bearer $OWNER" >/dev/null
VOIDED=$(pys "sum(1 for i in d if i['status']['state'] == 'void')")
if [ "$VOIDED" = "0" ]; then
  body -X POST "$B/api/v1/gyms/$IRON/invoices" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' \
    -d "{\"member_id\":\"$MEMBER_ID\",\"description\":\"Personal training block (billed in error)\",\"amount_minor\":24000,\"currency\":\"GBP\",\"due_on\":\"$(date +%F)\"}" >/dev/null
  WRONG_INV=$(pys "d['id']")
  body -X POST "$B/api/v1/gyms/$IRON/invoices/$WRONG_INV/void" -H "authorization: Bearer $OWNER" \
    -H 'content-type: application/json' -d '{"reason":"billed to the wrong member"}' >/dev/null
  echo "    one invoice raised then VOIDED — corrections are never edits"
else
  echo "    (a voided invoice already exists)"
fi

# --- a class timetable -------------------------------------------------------
# Four classes across the week, taught by the two staff accounts, with real
# capacities. A class is a WEEKLY SLOT (0 = Sunday), so there are no dates
# here — every Monday is derived from the slot at read time.
#
# One is deliberately small (Pilates, 12) so "nearly full" is reachable in a
# demo, and one is taught by the OWNER so the roster view is demonstrable from
# two different accounts.
CLASSES=$(body "$B/api/v1/gyms/$IRON/classes" -H "authorization: Bearer $OWNER" >/dev/null; pys "len(d)")


# The owner teaches one of them, so the roster view is reachable from two
# different staff accounts. `uid` is the same helper the pairing above uses.
OWNER_ID=$(uid "$OWNER")

if [ "${CLASSES:-0}" = "0" ]; then
  addclass() { # addclass <name> <weekday> <start> <mins> <cap> <instructor> <blurb>
    body -X POST "$B/api/v1/gyms/$IRON/classes" -H "authorization: Bearer $OWNER"       -H 'content-type: application/json'       -d "{\"name\":\"$1\",\"instructor_id\":\"$6\",\"weekday\":$2,\"starts_at\":\"$3\",\"duration_minutes\":$4,\"capacity\":$5,\"description\":\"$7\"}" >/dev/null
  }
  addclass "Zumba"                 1 "18:00:00" 45 20 "$TRAINER_ID" "Latin dance cardio. No experience needed."
  addclass "High Intensity Cardio" 2 "07:00:00" 30 20 "$TRAINER_ID" "Intervals. Bring a towel."
  addclass "Yoga"                  3 "19:00:00" 60 15 "$OWNER_ID"   "Slow flow and breath work."
  addclass "Pilates"               4 "18:30:00" 50 12 "$TRAINER_ID" "Core control on the mat."
  echo "    4 classes on the timetable (Zumba, HIIT, Yoga, Pilates)"

  # A couple of places taken, so occupancy is not a row of zeros. Booked by the
  # member for the NEXT occurrence of each, which the timetable read resolves.
  book_next() { # book_next <class-name> <token>
    body "$B/api/v1/gyms/$IRON/classes" -H "authorization: Bearer $2" >/dev/null
    local cid on
    cid=$(pys "next((c['class_id'] for c in d if c['name'] == '$1'), '')")
    on=$(pys "next((c['on_date'] for c in d if c['name'] == '$1'), '')")
    [ -n "$cid" ] || return 0
    body -X POST "$B/api/v1/gyms/$IRON/classes/$cid/bookings" -H "authorization: Bearer $2"       -H 'content-type: application/json'       -d "{\"id\":\"$("$PY" -c 'import uuid;print(uuid.uuid4())')\",\"on_date\":\"$on\"}" >/dev/null
  }
  book_next "Yoga" "$MEMBER"
  book_next "Pilates" "$MEMBER"
  echo "    member@ holds two places, so a booking is visible on Today"
else
  echo "    (the timetable already has classes)"
fi

