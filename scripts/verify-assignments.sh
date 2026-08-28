#!/usr/bin/env bash
# Programme assignment, end to end over HTTP.
#
# The load-bearing rule: a trainer may assign programmes TO THEIR OWN CLIENTS,
# and to nobody else. This is the first place the coach–athlete relationship is
# exercised as *authority* rather than visibility, so both directions get
# tested — the client it permits and the stranger it does not.
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
export SERVER_PORT=8094
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
./target/debug/server > /tmp/server-assignments.log 2>&1 &
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

echo "=== setup: gym, coach pair, one published + one draft programme ==="
OWNER_T=$(signup "as-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Assignment Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

T1_T=$(signup "as-t1-$S@example.com" "Tariq Trainer"); accept "$T1_T" "$(invite "as-t1-$S@example.com" '["trainer"]')"
M1_T=$(signup "as-m1-$S@example.com" "Mo Member");     accept "$M1_T" "$(invite "as-m1-$S@example.com" '["member"]')"
M2_T=$(signup "as-m2-$S@example.com" "Mia Member");    accept "$M2_T" "$(invite "as-m2-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); M1=$(uid "$M1_T"); M2=$(uid "$M2_T")

# T1 coaches M1 only. M2 is deliberately nobody's client.
# Pairing is a two-step handshake now (ADR-0034): the manager proposes and
# the trainer accepts. Direct pairing is gone, because the relationship hands
# the trainer that member's whole training history and they get asked first.
PAIR_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$M1\",\"coach_id\":\"$T1\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$PAIR_REQ/answer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"accept"' >/dev/null

# A published version to assign, and a draft that must be refused.
code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Strength A"}' >/dev/null
V_PUB=$(pyq "d['latest_version']['id']")
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V_PUB/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":1}' >/dev/null
# The review gate counts PRESCRIBED EXERCISES, not weeks: a week (or a
# workout) with nothing in it is not a trainable plan. Give it one.
AS_WEEK=$(pyq "d['id']")
AS_EX=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Back Squat","modality":{"kind":"repetitions"}}' >/dev/null; pyq "d['id']")
AS_WK=$(code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$AS_WEEK/workouts" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Day 1"}' >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$AS_WK/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$AS_EX\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5},\"rir\":2}}" >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V_PUB/transition" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '"submit_for_review"' >/dev/null
# The owner authored it, so a second person must approve: use a head coach.
HC_T=$(signup "as-hc-$S@example.com" "Hana Head"); accept "$HC_T" "$(invite "as-hc-$S@example.com" '["owner"]')"
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V_PUB/transition" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '"approve"' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V_PUB/transition" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '"publish"' >/dev/null

code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Draft Only"}' >/dev/null
V_DRAFT=$(pyq "d['latest_version']['id']")

echo ""
echo "=== 1. authority — the relationship, not the capacity ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "trainer assigns their OWN client -> 201" "$ST" "201"
A1=$(pyq "d['id']")
check "  pins the exact version" "$(pyq "d['program_version_id'] == '$V_PUB'")" "True"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M2\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "trainer assigns someone ELSE's member -> 403" "$ST" "403"

# Self-assignment used to be refused here; it is now allowed (see section 6) --
# a member who joined on their own had no coach and so no route onto any
# programme at all. What has NOT changed is the rule this check really exists
# for: a member has authority over their own training and nobody else's.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M2\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "a member assigning SOMEBODY ELSE -> 403" "$ST" "403"

# Reversed by ADR-0034. A manager writes and publishes the catalogue and
# decides who coaches whom; they do not reach past the trainer to prescribe.
# The owner being able to do everything made them the default prescriber for
# the whole gym simply because they could.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M2\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "a head coach who is not their coach -> 403" "$ST" "403"

# M2 is nobody's client, so the only route onto a programme is their own.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M2_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M2\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "  so M2 puts themselves on it -> 201" "$ST" "201"

echo ""
echo "=== 2. only published versions ==="
# By the athlete's own coach — otherwise the 403 fires first and this stops
# testing the published-only rule at all.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_DRAFT\",\"start_date\":\"$TODAY\"}")
check "assigning a draft -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "duplicate active assignment -> 409" "$ST" "409"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"2062-01-01\"}")
check "fat-fingered year -> 400" "$ST" "400"

echo ""
echo "=== 3. visibility ==="
ST=$(code "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M1_T")
check "member sees own assignment" "$(pyq "len(d)")" "1"
check "  with the programme name resolved" "$(pyq "d[0]['program_name']")" "Strength A"
check "  and the version number" "$(pyq "d[0]['version_number']")" "1"

ST=$(code "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T")
check "trainer sees their client's, not M2's" "$(pyq "sorted(a['athlete_id'] for a in d) == ['$M1']")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $HC_T")
check "head coach sees both" "$(pyq "len(d)")" "2"

OUT_T=$(signup "as-out-$S@example.com" "Ozzy Outsider")
ST=$(code "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $OUT_T")
check "outsider -> 404" "$ST" "404"

echo ""
echo "=== 4. withdrawal ==="
# Choosing a programme and stopping one are the same right, so a member may now
# end their own assignment -- including one their coach set. It is audited, so
# the coach sees it happened rather than finding a silently abandoned plan.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments/$A1/withdraw" -H "authorization: Bearer $M1_T")
check "member withdraws themselves -> 200" "$ST" "200"
check "  and the record names them, not their coach" "$(pyq "d['status']['withdrawn_by'] == '$M1'")" "True"

# Back on it, so the coach's own withdrawal is still exercised below.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "their coach puts them back on it -> 201" "$ST" "201"
A1=$(pyq "d['id']")

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments/$A1/withdraw" -H "authorization: Bearer $T1_T")
check "their coach withdraws -> 200" "$ST" "200"
check "  no longer active" "$(pyq "d['is_active']")" "False"
check "  records who" "$(pyq "d['status']['withdrawn_by'] == '$T1'")" "True"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments/$A1/withdraw" -H "authorization: Bearer $T1_T")
check "withdrawing twice -> 409" "$ST" "409"

# Re-assignment after withdrawal is the normal coaching cycle.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "re-assign after withdrawal -> 201" "$ST" "201"

# The withdrawn record is still there — history, not tombstones.
ST=$(code "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M1_T")
check "every withdrawal is kept as history" "$(pyq "sum(1 for a in d if not a['is_active'])")" "2"
check "  exactly one active" "$(pyq "sum(1 for a in d if a['is_active'])")" "1"

echo ""
echo "=== 5. audit trail ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
for a in program.assigned program_assignment.withdrawn; do
  check "recorded $a" "$(pyq "any(e['action']=='$a' for e in d)")" "True"
done

echo ""
echo "=== 6. solo training: a member puts THEMSELVES on a programme ==="
# The gap this closes: a member who joined through the open door has no coach,
# and assignment was coach-or-manager only. So they could read the whole
# library and train against none of it — the app tracked nothing for them, and
# the catalogue was a list of things they could not use.
#
# A brand-new walk-in: no coach, no assignment, nobody's client -- exactly the
# person the app used to track nothing for.
M4_T=$(signup "as-m4-$S@example.com" "Nia Newcomer"); accept "$M4_T" "$(invite "as-m4-$S@example.com" '["member"]')"
M4=$(uid "$M4_T")
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M4_T"   -H 'content-type: application/json'   -d "{\"athlete_id\":\"$M4\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "a coachless member may assign themselves" "$ST" "201"
SELF_ASG=$(pyq "d['id']")

# ...and nothing more than themselves. This is self-service, not a coaching
# right: if it leaked one step further it would be a member handing themselves
# authority over somebody else's training.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M4_T"   -H 'content-type: application/json'   -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_PUB\",\"start_date\":\"$TODAY\"}")
check "  but NOT somebody else" "$ST" "403"

# Every guard that made assignment safe still applies.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $M4_T"   -H 'content-type: application/json'   -d "{\"athlete_id\":\"$M4\",\"program_version_id\":\"$V_DRAFT\",\"start_date\":\"$TODAY\"}")
check "  and an unpublished version is still refused" "$ST" "400"

# Symmetrical: choosing a programme and stopping one are the same right.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments/$SELF_ASG/withdraw"   -H "authorization: Bearer $M4_T")
check "  they may take themselves off it again" "$ST" "200"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
