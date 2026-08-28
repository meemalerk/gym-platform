#!/usr/bin/env bash
# Unplanned sessions — training with no coach and no prescription (ADR-0035).
#
# The case this exists for: somebody on an Open Gym membership. They hold
# `gym_access` and nothing else, so no coach will ever be assigned to them, so
# nobody will ever prescribe them anything — and before this the app recorded
# precisely nothing for them. Every assertion below is about a member acting
# alone.
#
# Four claims get tested hardest:
#   1. A member with NO assignment can start, log, finish and read back.
#   2. The plan link is both-or-neither — in the domain AND in the database.
#   3. An unplanned session does not leak coaching authority: it is still only
#      the athlete who writes it, and still only the usual readers who see it.
#   4. It appears in the lists. The joins that name a workout are LEFT joins
#      now, and an inner one would have hidden these rows everywhere.
cd "$(dirname "$0")/.." || exit 1

export VTMP="target/verify-tmp"
mkdir -p "$VTMP"

if [ -z "${PY:-}" ]; then
  for candidate in python3 python py; do
    if command -v "$candidate" >/dev/null 2>&1 \
       && "$candidate" -c 'import sys; sys.exit(0 if sys.version_info[0] == 3 else 1)' >/dev/null 2>&1; then
      PY="$candidate"; break
    fi
  done
fi
if [ -z "${PY:-}" ]; then echo "no python3 on PATH — cannot parse API responses" >&2; exit 1; fi
export PY
export DATABASE_URL="postgres://gym:gym_dev_password@localhost:5455/gym"
export APP_DATABASE_URL="${APP_DATABASE_URL:-postgres://gym_app:gym_app_dev_password@localhost:5455/gym}"
export SERVER_PORT=8113
export SERVER_HOST=127.0.0.1
export JWT_SECRET="dev-only-insecure-secret-change-me-before-any-real-deployment"
export RUST_LOG=warn
B="http://127.0.0.1:$SERVER_PORT"

reap_stale_servers() {
  if command -v taskkill >/dev/null 2>&1; then
    taskkill //F //IM server.exe >/dev/null 2>&1 || true
  elif command -v pkill >/dev/null 2>&1; then
    pkill -9 -f "$PWD/target/debug/server" 2>/dev/null || true
  fi
}
reap_stale_servers

cargo build --bin server 2>&1 | tail -2 || exit 1
./target/debug/server > "$VTMP/server-unplanned.log" 2>&1 &
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
psql_one() { "$PY" - "$1" <<'PYEOF'
import os, subprocess, sys
sql = sys.argv[1]
out = subprocess.run(
    ["docker", "exec", "-i", "gym-postgres", "psql", "-U", "gym", "-d", "gym", "-tAc", sql],
    capture_output=True, text=True,
)
sys.stdout.write((out.stdout or out.stderr).strip())
PYEOF
}

S=$(date +%s%N); PW="correct horse battery staple"
NOW=$(date -u +%Y-%m-%dT%H:%M:%SZ)

signup() { code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$1\",\"password\":\"$PW\",\"display_name\":\"$2\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json; }
uid() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/b.json; }
# Open the door, walk through, SHUT IT AGAIN, then set standing. Leaving it
# open advertises this throwaway gym on the onboarding list of the next real
# person who signs up — which is exactly how that list reached 163 rows.
join_as() {
  code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d '{"open_registration":true}' >/dev/null
  code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $1" >/dev/null
  code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d '{"open_registration":false}' >/dev/null
  code -X PUT "$B/api/v1/gyms/$GYM/members/$(uid "$1")/capacities" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d "{\"capacities\":$2}" >/dev/null
}

echo "=== setup: a gym, an Open Gym plan, and a member with no coach ==="
OWNER_T=$(signup "un-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Unplanned Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

M_T=$(signup "un-member-$S@example.com" "Mo Member"); join_as "$M_T" '["member"]'
M=$(uid "$M_T")
OTHER_T=$(signup "un-other-$S@example.com" "Ola Other"); join_as "$OTHER_T" '["member"]'
TR_T=$(signup "un-trainer-$S@example.com" "Tariq Trainer"); join_as "$TR_T" '["trainer"]'

# An Open Gym plan: gym access and nothing else. This is the shape of the
# membership the whole feature is for, so the member is really subscribed to
# it rather than relying on the gym having no plans at all.
PLAN=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Open Gym","price_minor":4900,"currency":"GBP","interval":"monthly","grants":["gym_access"]}' >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$M\",\"plan_id\":\"$PLAN\",\"started_on\":\"$(date +%F)\"}" >/dev/null

BENCH=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Bench Press","modality":{"kind":"repetitions"}}' >/dev/null; jget "['id']" < $VTMP/b.json)
ROW=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Seated Row","modality":{"kind":"repetitions"}}' >/dev/null; jget "['id']" < $VTMP/b.json)

echo ""
echo "=== 1. the member has no coach and no assignment ==="
ST=$(code "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M_T")
check "assignments readable" "$ST" "200"
check "no assignment exists for them" "$(pyq "len([a for a in d if a['athlete_id']=='$M'])")" "0"
ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $M_T")
check "holds gym access" "$(pyq "any(e['feature']=='gym_access' for e in d['held'])")" "True"
check "does NOT hold coached programming" \
  "$(pyq "any(e['feature']=='coached_programming' for e in d['held'])")" "False"

echo ""
echo "=== 2. they can start a workout of their own anyway ==="
SESS=$(newid)
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$SESS\",\"title\":\"Push day\",\"started_at\":\"$NOW\"}")
check "started with no assignment" "$ST" "201"
check "carries the name they gave it" "$(pyq "d['title']")" "Push day"
check "no assignment on the record" "$(pyq "d['assignment_id'] is None")" "True"
check "no workout template either" "$(pyq "d['workout_template_id'] is None")" "True"
check "is open" "$(pyq "d['is_open']")" "True"

# The offline primitive (ADR-0008) has to hold on this path too, not just the
# assigned one — a phone in a basement replays whatever it queued.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$SESS\",\"title\":\"Push day\",\"started_at\":\"$NOW\"}")
check "replaying the id is a no-op, not a second session" "$ST" "200"

echo ""
echo "=== 3. the plan link is both-or-neither ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$(newid)\",\"workout_template_id\":\"$(newid)\",\"started_at\":\"$NOW\"}")
check "a workout with no assignment is refused" "$ST" "400"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$(newid)\",\"assignment_id\":\"$(newid)\",\"started_at\":\"$NOW\"}")
check "an assignment with no workout is refused" "$ST" "400"

# The database says the same thing independently of the application — the same
# belt-and-braces the programme lifecycle gets, because the app is not the only
# thing that can reach these tables.
#
# Only ONE direction reaches the CHECK by insert, and that is worth knowing
# rather than working around: `check_session_validity` is a BEFORE trigger, and
# Postgres runs those ahead of CHECK constraints — so an insert naming a
# non-existent assignment dies on "assignment not found" first. The direction
# below (a template with no assignment) takes the trigger's early return and
# lands on the constraint, which is the guard being tested.
check "the DB refuses half a plan link too" \
  "$(psql_one "INSERT INTO workout_sessions (id, gym_id, athlete_id, workout_template_id, started_at, status) VALUES (gen_random_uuid(), '$GYM', '$M', gen_random_uuid(), now(), 'in_progress')" | grep -c "workout_sessions_plan_link_whole")" \
  "1"

# The other direction needs a real assignment to get past that trigger, and
# this member has none by design — so assert the guard is INSTALLED and
# validated, which is what makes Postgres enforce it on every future write.
# Weaker than a refused insert, and named as weaker rather than dressed up.
check "the both-or-neither guard is installed" \
  "$(psql_one "SELECT count(*) FROM pg_constraint WHERE conname = 'workout_sessions_plan_link_whole' AND contype = 'c' AND convalidated")" \
  "1"
check "the title-only-on-unplanned guard is installed" \
  "$(psql_one "SELECT count(*) FROM pg_constraint WHERE conname = 'workout_sessions_title_is_for_open' AND contype = 'c' AND convalidated")" \
  "1"

echo ""
echo "=== 4. a name is bounded, and blank is no name ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"title\":\"$("$PY" -c 'print("x"*81)')\",\"started_at\":\"$NOW\"}")
check "81 characters is refused" "$ST" "400"
BLANK=$(newid)
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$BLANK\",\"title\":\"   \",\"started_at\":\"$NOW\"}")
check "a blank name is accepted" "$ST" "201"
check "...and stored as no name at all" "$(pyq "d['title'] is None")" "True"
NONAME=$(newid)
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$NONAME\",\"started_at\":\"$NOW\"}")
check "no name at all is fine" "$ST" "201"

echo ""
echo "=== 5. logging sets, with nothing prescribed to answer ==="
for n in 1 2 3; do
  ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SESS/sets" -H "authorization: Bearer $M_T" \
    -H 'content-type: application/json' \
    -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$BENCH\",\"set_number\":$n,\"performed\":{\"kind\":\"repetitions\",\"reps\":8,\"weight_kg\":60},\"rpe\":8}")
  check "logged bench set $n" "$ST" "201"
done
check "the set answers no prescription line" "$(pyq "d['template_exercise_id'] is None")" "True"

# A second exercise added mid-session — the whole point of building it yourself.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SESS/sets" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$ROW\",\"set_number\":1,\"performed\":{\"kind\":\"repetitions\",\"reps\":12,\"weight_kg\":40}}")
check "a second exercise can be added while training" "$ST" "201"

echo ""
echo "=== 6. it is still only the athlete who writes it ==="
# No assignment means no assignment to check ownership against, so this is the
# single guard left on the write path. Worth its own section.
#
# 400 rather than 403: the domain constructor refuses it, and the refusal is
# about the CONTENT of the request ("sets can only be logged into your own
# session") rather than the caller's standing. verify-execution.sh pins the
# same code for a coach doing this to an assigned session.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SESS/sets" -H "authorization: Bearer $OTHER_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$BENCH\",\"set_number\":9,\"performed\":{\"kind\":\"repetitions\",\"reps\":1}}")
check "another member cannot log into it -> 400" "$ST" "400"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SESS/sets" -H "authorization: Bearer $TR_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$BENCH\",\"set_number\":9,\"performed\":{\"kind\":\"repetitions\",\"reps\":1}}")
check "a trainer cannot log into it either -> 400" "$ST" "400"
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions/$SESS" -H "authorization: Bearer $OTHER_T")
check "an unrelated member cannot even read it" "$ST" "404"

echo ""
echo "=== 7. it appears in the lists (the LEFT JOIN claim) ==="
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M_T")
check "list readable" "$ST" "200"
check "the unplanned session is in it" "$(pyq "any(x['id']=='$SESS' for x in d)")" "True"
check "with its name" "$(pyq "[x['title'] for x in d if x['id']=='$SESS'][0]")" "Push day"
check "and its set count" "$(pyq "[x['set_count'] for x in d if x['id']=='$SESS'][0]")" "4"
check "no programme name is invented for it" \
  "$(pyq "[x['program_name'] for x in d if x['id']=='$SESS'][0] is None")" "True"
check "no workout name is invented either" \
  "$(pyq "[x['workout_name'] for x in d if x['id']=='$SESS'][0] is None")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions/$SESS" -H "authorization: Bearer $M_T")
check "detail readable by its athlete" "$ST" "200"
check "detail carries the sets" "$(pyq "len(d['sets'])")" "4"

echo ""
echo "=== 8. the history it produces is ordinary history ==="
# Nothing downstream reads the prescription, which is why this works at all —
# so est-1RM, exercise history and progress must not care that there was none.
ST=$(code "$B/api/v1/gyms/$GYM/exercises/$BENCH/history?athlete_id=$M" -H "authorization: Bearer $M_T")
check "exercise history readable" "$ST" "200"
check "the unplanned session is in it" "$(pyq "any(e['session_id']=='$SESS' for e in d)")" "True"
check "with the sets attached" \
  "$(pyq "sum(len(e['sets']) for e in d if e['session_id']=='$SESS')")" "3"

echo ""
echo "=== 9. finishing, and staying finished ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SESS/finish" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' -d "{\"outcome\":\"completed\",\"ended_at\":\"$NOW\"}")
check "finished" "$ST" "200"
check "reports a duration" "$(pyq "d['duration_seconds'] is not None")" "True"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SESS/sets" -H "authorization: Bearer $M_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$(newid)\",\"exercise_id\":\"$BENCH\",\"set_number\":9,\"performed\":{\"kind\":\"repetitions\",\"reps\":1}}")
check "no more sets go into a finished session" "$([ "$ST" = "400" ] || [ "$ST" = "409" ] && echo refused)" "refused"

echo ""
echo "=== 10. the registration door was left shut ==="
# Pinned here because three suites once left it open, and an open door on a
# throwaway gym is advertised to the next real person who signs up.
check "door closed" \
  "$(psql_one "SELECT open_registration FROM gyms WHERE id = '$GYM'")" "f"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
