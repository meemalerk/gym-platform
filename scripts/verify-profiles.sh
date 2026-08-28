#!/usr/bin/env bash
# Person-owned profiles, end to end over HTTP.
#
# These tables sit OUTSIDE row-level security by design, which makes the service
# gate the ONLY wall. So the boundary gets tested hardest: a coach may read
# their client's limitations; a fellow member must get 404 and never learn the
# athlete trains here.
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
export SERVER_PORT=8092
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
./target/debug/server > /tmp/server-profiles.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then PASS=$((PASS+1));
          else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi; }
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
pyq() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.load(open('$VTMP/b.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/b.json -w "%{http_code}" "$@"; }

S=$(date +%s%N); PW="correct horse battery staple"

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

echo "=== setup ==="
OWNER_T=$(signup "pr-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Profile Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)
T1_T=$(signup "pr-t1-$S@example.com" "Tariq Trainer"); accept "$T1_T" "$(invite "pr-t1-$S@example.com" '["trainer"]')"
M1_T=$(signup "pr-m1-$S@example.com" "Mo Member");     accept "$M1_T" "$(invite "pr-m1-$S@example.com" '["member"]')"
M2_T=$(signup "pr-m2-$S@example.com" "Mia Member");    accept "$M2_T" "$(invite "pr-m2-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); M1=$(uid "$M1_T")
# Pairing is a two-step handshake (ADR-0034): the manager proposes, the named
# trainer accepts. Direct pairing is gone — the relationship hands the trainer
# that member's whole training history, so they get asked first.
PAIR_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$M1\",\"coach_id\":\"$T1\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$PAIR_REQ/answer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"accept"' >/dev/null

echo ""
echo "=== 1. own profiles ==="
ST=$(code "$B/api/v1/me/profiles" -H "authorization: Bearer $M1_T")
check "empty profiles -> 200" "$ST" "200"
check "  both null (an invitation, not an error)" "$(pyq "d['athlete'] is None and d['trainer'] is None")" "True"

ST=$(code -X PUT "$B/api/v1/me/profiles/athlete" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d '{"goals":"Squat 100 kg, feel human again.","training_age_months":18,"limitations":"Left knee: no deep lunges.","date_of_birth":"1994-03-12"}')
check "save athlete profile -> 200" "$ST" "200"
check "  goals round-trip" "$(pyq "d['goals']")" "Squat 100 kg, feel human again."

ST=$(code "$B/api/v1/me/profiles" -H "authorization: Bearer $M1_T")
check "  persisted" "$(pyq "d['athlete']['training_age_months']")" "18"

# Full replace: an omitted field CLEARS. Merge semantics are where "I cleared
# that box" quietly stops working.
ST=$(code -X PUT "$B/api/v1/me/profiles/athlete" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"goals":"Squat 100 kg."}')
check "replace clears omitted fields" "$(pyq "d['limitations'] is None")" "True"

echo ""
echo "=== 2. validation ==="
ST=$(code -X PUT "$B/api/v1/me/profiles/athlete" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"date_of_birth":"2030-01-01"}')
check "future birthday -> 400" "$ST" "400"
ST=$(code -X PUT "$B/api/v1/me/profiles/athlete" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"date_of_birth":"2020-01-01"}')
check "under-13 birthday -> 400" "$ST" "400"
ST=$(code -X PUT "$B/api/v1/me/profiles/athlete" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"training_age_months":2000}')
check "absurd training age -> 400" "$ST" "400"

ST=$(code -X PUT "$B/api/v1/me/profiles/trainer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d '{"headline":"Strength coach","bio":"Ten years under the bar.","certifications":["  CSCS ","cscs","","Precision Nutrition L1"],"specialties":["Powerlifting"]}')
check "save trainer profile -> 200" "$ST" "200"
check "  labels trimmed and case-deduplicated" "$(pyq "d['certifications']")" "['CSCS', 'Precision Nutrition L1']"

echo ""
echo "=== 3. rename ==="
ST=$(code -X PATCH "$B/api/v1/me" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"display_name":"  Mo Trains  "}')
check "rename -> 200" "$ST" "200"
check "  trimmed" "$(pyq "d['display_name']")" "Mo Trains"
ST=$(code "$B/api/v1/me" -H "authorization: Bearer $M1_T")
check "  visible in /me" "$(pyq "d['user']['display_name']")" "Mo Trains"
# The name feeds rosters and coaching lists — a rename must propagate.
ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $T1_T")
check "  and in the coach's client list" "$(pyq "d[0]['athlete_name']")" "Mo Trains"
ST=$(code -X PATCH "$B/api/v1/me" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"display_name":"   "}')
check "blank name -> 400" "$ST" "400"

echo ""
echo "=== 4. the boundary — who reads whose ==="
ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/athlete-profile" -H "authorization: Bearer $T1_T")
check "their coach reads the profile -> 200" "$ST" "200"
check "  including what matters" "$(pyq "d['goals']")" "Squat 100 kg."

ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/athlete-profile" -H "authorization: Bearer $OWNER_T")
check "a manager reads it -> 200" "$ST" "200"

ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/athlete-profile" -H "authorization: Bearer $M2_T")
check "a fellow member -> 404, never confirmation" "$ST" "404"

OUT_T=$(signup "pr-out-$S@example.com" "Ozzy Outsider")
ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/athlete-profile" -H "authorization: Bearer $OUT_T")
check "an outsider -> 404" "$ST" "404"

# Standing over someone who never filled it in: blank fields, not an error.
M2=$(uid "$M2_T")
ST=$(code "$B/api/v1/gyms/$GYM/members/$M2/athlete-profile" -H "authorization: Bearer $OWNER_T")
check "unfilled profile reads as empty -> 200" "$ST" "200"
check "  all null" "$(pyq "all(v is None for v in d.values())")" "True"

echo ""
echo "=== 5. body measurements ==="
TODAY=$(date +%F); LASTWEEK=$(date -d "-7 days" +%F)

ST=$(code -X PUT "$B/api/v1/me/measurements/$LASTWEEK" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"weight_kg":82.0,"waist_cm":88.5}')
check "backdated weigh-in -> 200" "$ST" "200"
ST=$(code -X PUT "$B/api/v1/me/measurements/$TODAY" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"weight_kg":81.2,"body_fat_percent":19.5}')
check "today's weigh-in -> 200" "$ST" "200"

# Re-entry the same day is a correction, not a second fact.
ST=$(code -X PUT "$B/api/v1/me/measurements/$TODAY" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"weight_kg":81.4}')
check "same-day re-entry replaces -> 200" "$ST" "200"
ST=$(code "$B/api/v1/me/measurements" -H "authorization: Bearer $M1_T")
check "  still two rows" "$(pyq "len(d)")" "2"
check "  newest first, corrected value" "$(pyq "d[0]['weight_kg']")" "81.4"
check "  replace cleared the omitted field" "$(pyq "d[0]['body_fat_percent'] is None")" "True"

ST=$(code -X PUT "$B/api/v1/me/measurements/$TODAY" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"weight_kg":785}')
check "slipped digit (785 kg) -> 400" "$ST" "400"
ST=$(code -X PUT "$B/api/v1/me/measurements/2099-01-01" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"weight_kg":80}')
check "future date -> 400" "$ST" "400"
ST=$(code -X PUT "$B/api/v1/me/measurements/$TODAY" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '{"notes":"felt great"}')
check "a row with no numbers -> 400" "$ST" "400"

echo ""
echo "=== 6. measurement visibility & the eraser ==="
ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/measurements" -H "authorization: Bearer $T1_T")
check "their coach reads the trend -> 200" "$ST" "200"
check "  both rows" "$(pyq "len(d)")" "2"
ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/measurements" -H "authorization: Bearer $M2_T")
check "a fellow member -> 404" "$ST" "404"

ST=$(code -X DELETE "$B/api/v1/me/measurements/$LASTWEEK" -H "authorization: Bearer $M1_T")
check "delete own entry -> 204" "$ST" "204"
ST=$(code -X DELETE "$B/api/v1/me/measurements/$LASTWEEK" -H "authorization: Bearer $M1_T")
check "delete twice -> 404" "$ST" "404"
ST=$(code "$B/api/v1/me/measurements" -H "authorization: Bearer $M1_T")
check "one row remains" "$(pyq "len(d)")" "1"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
