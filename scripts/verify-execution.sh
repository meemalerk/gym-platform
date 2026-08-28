#!/usr/bin/env bash
# Workout execution, end to end over HTTP.
#
# Three claims get tested hardest, because everything downstream rests on them:
#   1. Only the athlete writes their own history — not even their coach.
#   2. Inserts are idempotent on the client id (the offline-sync primitive).
#   3. A finished session is frozen, and performed sets have no update path.
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
export SERVER_PORT=8093
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
./target/debug/server > /tmp/server-execution.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then PASS=$((PASS+1));
          else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi; }
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
pyq() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.load(open('$VTMP/b.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/b.json -w "%{http_code}" "$@"; }
newid() { "$PY" -c "import uuid;print(uuid.uuid4())"; }

S=$(date +%s%N); PW="correct horse battery staple"; TODAY=$(date +%F)
NOW=$(date -u +%Y-%m-%dT%H:%M:%SZ)

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

echo "=== setup: published programme, coached member, assignment ==="
OWNER_T=$(signup "ex-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Execution Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)
HC_T=$(signup "ex-hc-$S@example.com" "Hana Head"); accept "$HC_T" "$(invite "ex-hc-$S@example.com" '["owner"]')"
T1_T=$(signup "ex-t1-$S@example.com" "Tariq Trainer"); accept "$T1_T" "$(invite "ex-t1-$S@example.com" '["trainer"]')"
M1_T=$(signup "ex-m1-$S@example.com" "Mo Member"); accept "$M1_T" "$(invite "ex-m1-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); M1=$(uid "$M1_T")

# Pairing is a two-step handshake (ADR-0034): the manager proposes, the named
# trainer accepts. Direct pairing is gone — the relationship hands the trainer
# that member's whole training history, so they get asked first.
PAIR_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$M1\",\"coach_id\":\"$T1\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$PAIR_REQ/answer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"accept"' >/dev/null

SQUAT=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Back Squat","modality":{"kind":"repetitions"}}' >/dev/null; jget "['id']" < $VTMP/b.json)

code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Strength"}' >/dev/null
V=$(pyq "d['latest_version']['id']")
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":1}' >/dev/null
WEEK=$(pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$WEEK/workouts" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Lower A"}' >/dev/null
WORKOUT=$(pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$WORKOUT/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$SQUAT\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5}}}" >/dev/null
TEMPLATE_EX=$(pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V/transition" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V/transition" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '"approve"' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V/transition" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '"publish"' >/dev/null

code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V\",\"start_date\":\"$TODAY\"}" >/dev/null
ASSIGNMENT=$(pyq "d['id']")

echo ""
echo "=== 1. starting a session ==="
SID=$(newid)
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SID\",\"assignment_id\":\"$ASSIGNMENT\",\"workout_template_id\":\"$WORKOUT\",\"started_at\":\"$NOW\"}")
check "athlete starts their session -> 201" "$ST" "201"
check "  open" "$(pyq "d['is_open']")" "True"

# The offline-sync primitive: same request again is a no-op, not an error.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SID\",\"assignment_id\":\"$ASSIGNMENT\",\"workout_template_id\":\"$WORKOUT\",\"started_at\":\"$NOW\"}")
check "replaying the same id -> 200, not 409" "$ST" "200"

# Not even their coach starts a session for them.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"assignment_id\":\"$ASSIGNMENT\",\"workout_template_id\":\"$WORKOUT\",\"started_at\":\"$NOW\"}")
check "coach starts athlete's session -> 403" "$ST" "403"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"assignment_id\":\"$ASSIGNMENT\",\"workout_template_id\":\"$WORKOUT\",\"started_at\":\"2099-01-01T00:00:00Z\"}")
check "session from the future -> 400" "$ST" "400"

echo ""
echo "=== 2. logging sets ==="
SET1=$(newid)
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/sets" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SET1\",\"exercise_id\":\"$SQUAT\",\"template_exercise_id\":\"$TEMPLATE_EX\",\"set_number\":1,\"performed\":{\"kind\":\"repetitions\",\"reps\":5,\"weight_kg\":60},\"rpe\":7}")
check "log set 1 -> 201" "$ST" "201"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/sets" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SET1\",\"exercise_id\":\"$SQUAT\",\"template_exercise_id\":\"$TEMPLATE_EX\",\"set_number\":1,\"performed\":{\"kind\":\"repetitions\",\"reps\":5,\"weight_kg\":60},\"rpe\":7}")
check "replaying the same set id -> 201 (no duplicate)" "$ST" "201"

# A failed set is history, not an error.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/sets" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$SQUAT\",\"set_number\":2,\"performed\":{\"kind\":\"repetitions\",\"reps\":0,\"weight_kg\":100},\"rpe\":10}")
check "a failed set (0 reps) -> 201" "$ST" "201"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/sets" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$SQUAT\",\"set_number\":3,\"performed\":{\"kind\":\"repetitions\",\"reps\":5}}")
check "coach logs into athlete's session -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/sets" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$SQUAT\",\"set_number\":3,\"performed\":{\"kind\":\"repetitions\",\"reps\":9999}}")
check "absurd reps -> 400" "$ST" "400"

echo ""
echo "=== 3. reading ==="
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions/$SID" -H "authorization: Bearer $M1_T")
check "athlete reads own session -> 200" "$ST" "200"
check "  two sets" "$(pyq "len(d['sets'])")" "2"
check "  weight survived as a decimal" "$(pyq "d['sets'][0]['performed']['weight_kg']")" "60.0"

ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions/$SID" -H "authorization: Bearer $T1_T")
check "their coach reads it -> 200" "$ST" "200"

OUT_T=$(signup "ex-out-$S@example.com" "Ozzy Outsider")
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions/$SID" -H "authorization: Bearer $OUT_T")
check "outsider -> 404" "$ST" "404"

ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $T1_T")
check "coach lists their client's sessions" "$(pyq "len(d)")" "1"
check "  with set count" "$(pyq "d[0]['set_count']")" "2"
check "  and programme name" "$(pyq "d[0]['program_name']")" "Strength"

echo ""
echo "=== 4. finishing freezes ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/finish" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"completed"')
check "coach finishes athlete's session -> 403" "$ST" "403"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/finish" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '"completed"')
check "athlete completes -> 200" "$ST" "200"
check "  closed" "$(pyq "d['is_open']")" "False"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/finish" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '"completed"')
check "finishing twice -> 409" "$ST" "409"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/sets" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$SQUAT\",\"set_number\":4,\"performed\":{\"kind\":\"repetitions\",\"reps\":5}}")
check "sets into a completed session -> 400" "$ST" "400"

# The role itself cannot rewrite history. Asserted per table because migration
# 0003's default privileges silently grant full DML to every new table — a
# narrow GRANT is additive, and only an explicit REVOKE (migration 0012)
# actually narrows. This check is what caught that.
CAN_UPDATE=$(docker exec -i -e PGPASSWORD=gym_app_dev_password gym-postgres \
  psql -U gym_app -h 127.0.0.1 -d gym -tAq \
  -c "SELECT has_table_privilege('gym_app', 'performed_sets', 'UPDATE') OR has_table_privilege('gym_app', 'performed_sets', 'DELETE');")
check "app role holds no UPDATE/DELETE on performed_sets" "$CAN_UPDATE" "f"

for table in workout_sessions coach_relationships program_assignments; do
  CAN_DELETE=$(docker exec -i -e PGPASSWORD=gym_app_dev_password gym-postgres \
    psql -U gym_app -h 127.0.0.1 -d gym -tAq \
    -c "SELECT has_table_privilege('gym_app', '$table', 'DELETE');")
  check "app role holds no DELETE on $table" "$CAN_DELETE" "f"
done

echo ""
echo "=== 5. exercise history ==="
ST=$(code "$B/api/v1/gyms/$GYM/exercises/$SQUAT/history" -H "authorization: Bearer $M1_T")
check "athlete reads own history -> 200" "$ST" "200"
check "  one session entry" "$(pyq "len(d)")" "1"
check "  both sets, in order" "$(pyq "[s['set_number'] for s in d[0]['sets']]")" "[1, 2]"
check "  session status included" "$(pyq "d[0]['session_status']")" "completed"

ST=$(code "$B/api/v1/gyms/$GYM/exercises/$SQUAT/history?athlete_id=$M1" -H "authorization: Bearer $T1_T")
check "their coach reads it -> 200" "$ST" "200"
check "  same sets" "$(pyq "len(d[0]['sets'])")" "2"

ST=$(code "$B/api/v1/gyms/$GYM/exercises/$SQUAT/history?athlete_id=$M1" -H "authorization: Bearer $OUT_T")
check "outsider -> 404" "$ST" "404"

# A second member has no standing over M1 — and must not learn they train here.
M2_T=$(signup "ex-m2-$S@example.com" "Mia Member"); accept "$M2_T" "$(invite "ex-m2-$S@example.com" '["member"]')"
ST=$(code "$B/api/v1/gyms/$GYM/exercises/$SQUAT/history?athlete_id=$M1" -H "authorization: Bearer $M2_T")
check "another member probing -> 404" "$ST" "404"
ST=$(code "$B/api/v1/gyms/$GYM/exercises/$SQUAT/history" -H "authorization: Bearer $M2_T")
check "their own (empty) history -> 200" "$ST" "200"
check "  and it is empty" "$(pyq "len(d)")" "0"

echo ""
echo "=== 6. audit ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
for a in workout_session.started workout_session.completed; do
  check "recorded $a" "$(pyq "any(e['action']=='$a' for e in d)")" "True"
done

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
