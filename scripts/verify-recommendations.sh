#!/usr/bin/env bash
# Recommendations, end to end over HTTP.
#
# The rule is deterministic, so the test can be exact: a lift goal recommends
# strength programmes and strength coaches — and nothing else, nobody already
# involved, and nothing without evidence to cite.
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
export SERVER_PORT=8089
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
./target/debug/server > /tmp/server-recs.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then PASS=$((PASS+1));
          else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi; }
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
pyq() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.load(open('$VTMP/b.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/b.json -w "%{http_code}" "$@"; }

S=$(date +%s%N); PW="correct horse battery staple"; TODAY=$(date +%F)

signup() { code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$1\",\"password\":\"$PW\",\"display_name\":\"$2\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json; }
uid() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/b.json; }
# Invitations are gone (ADR-0031). People join through the open door and
# somebody who runs the gym sets their standing, so these two helpers keep the
# old call shape — `accept "$TOK" "$(invite addr '["trainer"]')"` — over the
# new mechanism. `invite` now just carries the capacities through; `accept`
# joins and then promotes. The end state is what the invitation used to give:
# `set_capacities` REPLACES, so join-as-member then set `["trainer"]` leaves
# them holding exactly trainer.
invite() { printf '%s' "$2"; }
accept() {
  # Opening a door that is already open is a no-op, and doing it here keeps the
  # helper self-contained: a suite that provisions a user gets a working user
  # without having to know the door exists.
  code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d '{"open_registration":true}' >/dev/null
  code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $1" >/dev/null
  # Shut the door again straight away.
  #
  # These gyms are throwaways, but the flag is not: onboarding lists every
  # gym advertising an open door, so a suite that leaves one open puts a
  # "Solo Box 1787848112219154700" in front of the next real person who
  # signs up. Nineteen suites doing that is why that list had 163 rows.
  # Capacities below do not need the door, so this is the moment to close it.
  code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d '{"open_registration":false}' >/dev/null
  code -X PUT "$B/api/v1/gyms/$GYM/members/$(uid "$1")/capacities" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d "{\"capacities\":$2}" >/dev/null; }
publish_program() { # publish_program <name> <focus> -> version id
  code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OWNER_T" \
    -H 'content-type: application/json' -d "{\"name\":\"$1\",\"focus\":\"$2\"}" >/dev/null
  local v; v=$(pyq "d['latest_version']['id']")
  code -X POST "$B/api/v1/gyms/$GYM/program-versions/$v/weeks" -H "authorization: Bearer $OWNER_T" \
    -H 'content-type: application/json' -d '{"week_number":1}' >/dev/null
  # The review gate counts PRESCRIBED EXERCISES, not weeks: an empty week is
  # not a trainable plan. $SQUAT is created before the first call below.
  local wkid; wkid=$(pyq "d['id']")
  local woid; woid=$(code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$wkid/workouts" -H "authorization: Bearer $OWNER_T" \
    -H 'content-type: application/json' -d '{"day_number":1,"name":"Day 1"}' >/dev/null; pyq "d['id']")
  code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$woid/exercises" -H "authorization: Bearer $OWNER_T" \
    -H 'content-type: application/json' \
    -d "{\"exercise_id\":\"$SQUAT\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5},\"rir\":2}}" >/dev/null
  code -X POST "$B/api/v1/gyms/$GYM/program-versions/$v/transition" -H "authorization: Bearer $OWNER_T" \
    -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
  code -X POST "$B/api/v1/gyms/$GYM/program-versions/$v/transition" -H "authorization: Bearer $HC_T" \
    -H 'content-type: application/json' -d '"approve"' >/dev/null
  code -X POST "$B/api/v1/gyms/$GYM/program-versions/$v/transition" -H "authorization: Bearer $HC_T" \
    -H 'content-type: application/json' -d '"publish"' >/dev/null
  echo "$v"
}

echo "=== setup: programmes with foci, coaches with specialties, a lift goal ==="
OWNER_T=$(signup "rc-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Rec Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)
HC_T=$(signup "rc-hc-$S@example.com" "Hana Head"); accept "$HC_T" "$(invite "rc-hc-$S@example.com" '["owner"]')"
T2_T=$(signup "rc-t2-$S@example.com" "Yara Yoga"); accept "$T2_T" "$(invite "rc-t2-$S@example.com" '["trainer"]')"
M1_T=$(signup "rc-m1-$S@example.com" "Mo Member");  accept "$M1_T" "$(invite "rc-m1-$S@example.com" '["member"]')"
M2_T=$(signup "rc-m2-$S@example.com" "Mia Member"); accept "$M2_T" "$(invite "rc-m2-$S@example.com" '["member"]')"
M1=$(uid "$M1_T"); HC=$(uid "$HC_T")

# Specialties: the head coach speaks strength; the other trainer does not.
code -X PUT "$B/api/v1/me/profiles/trainer" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' \
  -d '{"headline":"Strength coach","specialties":["Powerlifting","Beginners"]}' >/dev/null
code -X PUT "$B/api/v1/me/profiles/trainer" -H "authorization: Bearer $T2_T" \
  -H 'content-type: application/json' \
  -d '{"headline":"Mobility","specialties":["Prenatal yoga"]}' >/dev/null

SQUAT=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Back Squat","modality":{"kind":"repetitions"}}' >/dev/null; jget "['id']" < $VTMP/b.json)

V_STRENGTH=$(publish_program "Strength A" "strength")
publish_program "Bulk Season" "hypertrophy" >/dev/null
publish_program "Base Camp" "general" >/dev/null

code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"exercise_est_1rm\",\"exercise_id\":\"$SQUAT\",\"baseline_kg\":80,\"target_kg\":100}}" >/dev/null

echo ""
echo "=== 1. the rule, exactly ==="
ST=$(code "$B/api/v1/gyms/$GYM/recommendations" -H "authorization: Bearer $M1_T")
check "recommendations -> 200" "$ST" "200"
check "  exactly one programme suggested" "$(pyq "len(d['programs'])")" "1"
check "  the strength one" "$(pyq "d['programs'][0]['name']")" "Strength A"
check "  hypertrophy and general stayed out" "$(pyq "any(p['name'] in ('Bulk Season','Base Camp') for p in d['programs'])")" "False"
check "  the reason names the goal" "$(pyq "'100 kg' in d['programs'][0]['because']")" "True"

check "  exactly one coach suggested" "$(pyq "len(d['trainers'])")" "1"
check "  the strength coach" "$(pyq "d['trainers'][0]['display_name']")" "Hana Head"
check "  the reason quotes the specialty" "$(pyq "'Powerlifting' in d['trainers'][0]['because']")" "True"
check "  yoga did not match a lift goal" "$(pyq "any(t['display_name']=='Yara Yoga' for t in d['trainers'])")" "False"

echo ""
echo "=== 2. suggestions retire when acted on ==="
# Pairing is a handshake now (ADR-0034), and note WHO proposes: a manager may
# not propose themselves as somebody's coach, so the owner names the head coach
# and the head coach accepts.
REC_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$M1\",\"coach_id\":\"$HC\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$REC_REQ/answer" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '"accept"' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_STRENGTH\",\"start_date\":\"$TODAY\"}" >/dev/null

ST=$(code "$B/api/v1/gyms/$GYM/recommendations" -H "authorization: Bearer $M1_T")
check "assigned programme no longer suggested" "$(pyq "len(d['programs'])")" "0"
check "your own coach no longer suggested" "$(pyq "len(d['trainers'])")" "0"

echo ""
echo "=== 3. no goals, no guesses ==="
ST=$(code "$B/api/v1/gyms/$GYM/recommendations" -H "authorization: Bearer $M2_T")
check "goal-less member gets empty lists -> 200" "$ST" "200"
check "  no programmes" "$(pyq "len(d['programs'])")" "0"
check "  no coaches" "$(pyq "len(d['trainers'])")" "0"

OUT_T=$(signup "rc-out-$S@example.com" "Ozzy Outsider")
ST=$(code "$B/api/v1/gyms/$GYM/recommendations" -H "authorization: Bearer $OUT_T")
check "outsider -> 404" "$ST" "404"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
