#!/usr/bin/env bash
# Goals, end to end over HTTP.
#
# The novel authority rule: a member may set their OWN goals (deciding what you
# chase is yours), a coach may set goals for clients, and a stranger may touch
# nobody's. Both directions of that line get tested.
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
export SERVER_PORT=8091
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
./target/debug/server > /tmp/server-goals.log 2>&1 &
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
FUTURE=$(date -d "+90 days" +%F)

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
OWNER_T=$(signup "gl-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Goal Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)
T1_T=$(signup "gl-t1-$S@example.com" "Tariq Trainer"); accept "$T1_T" "$(invite "gl-t1-$S@example.com" '["trainer"]')"
M1_T=$(signup "gl-m1-$S@example.com" "Mo Member");     accept "$M1_T" "$(invite "gl-m1-$S@example.com" '["member"]')"
M2_T=$(signup "gl-m2-$S@example.com" "Mia Member");    accept "$M2_T" "$(invite "gl-m2-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); M1=$(uid "$M1_T"); M2=$(uid "$M2_T")
# Pairing is a two-step handshake (ADR-0034): the manager proposes, the named
# trainer accepts. Direct pairing is gone — the relationship hands the trainer
# that member's whole training history, so they get asked first.
PAIR_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$M1\",\"coach_id\":\"$T1\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$PAIR_REQ/answer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"accept"' >/dev/null
SQUAT=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Back Squat","modality":{"kind":"repetitions"}}' >/dev/null; jget "['id']" < $VTMP/b.json)

echo ""
echo "=== 1. who may set whose ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"bodyweight\",\"baseline_kg\":82,\"target_kg\":78},\"target_date\":\"$FUTURE\"}")
check "member sets their OWN goal -> 201" "$ST" "201"
G1=$(pyq "d['id']")

ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"exercise_est_1rm\",\"exercise_id\":\"$SQUAT\",\"baseline_kg\":85,\"target_kg\":100}}")
check "their coach sets a lift goal -> 201" "$ST" "201"
G2=$(pyq "d['id']")

ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M2_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"bodyweight\",\"baseline_kg\":82,\"target_kg\":78}}")
check "a fellow member sets M1's goal -> 403" "$ST" "403"

echo ""
echo "=== 2. what a goal refuses ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"bodyweight\",\"baseline_kg\":82,\"target_kg\":82}}")
check "target equal to baseline -> 400" "$ST" "400"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"exercise_est_1rm\",\"exercise_id\":\"$SQUAT\",\"baseline_kg\":100,\"target_kg\":90}}")
check "a lift target below baseline -> 400" "$ST" "400"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"exercise_est_1rm\",\"exercise_id\":\"00000000-0000-0000-0000-000000000000\",\"baseline_kg\":85,\"target_kg\":100}}")
check "a goal on a nonexistent exercise -> 404" "$ST" "404"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"metric\":{\"kind\":\"bodyweight\",\"baseline_kg\":82,\"target_kg\":78},\"target_date\":\"2035-01-01\"}")
check "a dream-length deadline -> 400" "$ST" "400"

echo ""
echo "=== 3. visibility ==="
ST=$(code "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T")
check "member sees their goals" "$(pyq "len(d)")" "2"
ST=$(code "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $T1_T")
check "coach sees their client's" "$(pyq "len(d)")" "2"
check "  with the athlete named" "$(pyq "d[0]['athlete_name']")" "Mo Member"
ST=$(code "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M2_T")
check "the other member sees none of them" "$(pyq "len(d)")" "0"

echo ""
echo "=== 4. closing ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals/$G2/close" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"achieved"')
check "coach confirms the lift goal -> 200" "$ST" "200"
check "  records the confirmer" "$(pyq "d['status']['confirmed_by'] == '$T1'")" "True"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals/$G1/close" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '"abandoned"')
check "member abandons their own -> 200" "$ST" "200"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/goals/$G1/close" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '"achieved"')
check "closing twice -> 409" "$ST" "409"

# Closed goals stay visible — the coaching story keeps its record.
ST=$(code "$B/api/v1/gyms/$GYM/goals" -H "authorization: Bearer $M1_T")
check "closed goals remain listed" "$(pyq "len(d)")" "2"
check "  none active" "$(pyq "sum(1 for g in d if g['is_active'])")" "0"

echo ""
echo "=== 5. audit ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
for a in goal.created goal.achieved goal.abandoned; do
  check "recorded $a" "$(pyq "any(e['action']=='$a' for e in d)")" "True"
done

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
