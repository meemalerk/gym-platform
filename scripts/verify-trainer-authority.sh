#!/usr/bin/env bash
# Trainer authority (ADR-0024), end to end over HTTP.
#
# The claim under test is one sentence: **a trainer proposes, a head coach
# commits.** Everything below is that sentence, split into the places it has to
# hold.
#
#   authoring    write a programme, add content, submit it — coach-level,
#                because a draft binds nobody.
#   publishing   approve, publish, archive — head coach, because these are the
#                moves athletes actually feel.
#   catalogue    name a movement — coach-level, and usable IMMEDIATELY, or the
#                trainer is blocked mid-programme. Promotion stays curated,
#                because a duplicate exercise_id permanently splits an
#                athlete's estimated-1RM history (ADR-0018) and no later edit
#                can rejoin it.
#   assignment   put your OWN client on a published version. This one was
#                already correct on the server and broken only in the client;
#                it is pinned here so it stays correct on both sides.
#
# The negative cases matter more than the positive ones. An authority model
# that only proves what it permits has not been tested.
cd "$(dirname "$0")/.." || exit 1

export VTMP="target/verify-tmp"
mkdir -p "$VTMP"

# `python3` is not a usable name everywhere: on Windows it resolves to the
# Microsoft Store stub, which prints nothing and exits 0 — so every jget
# returned empty and the suite reported passes it never ran. Resolve a real
# interpreter once, here, and call THAT.
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
export SERVER_PORT=8102
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
./target/debug/server > /tmp/server-trainer.log 2>&1 &
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
mkex() { code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "{\"name\":\"$2\",\"modality\":{\"kind\":\"repetitions\"}}"; }
move() { code -X POST "$B/api/v1/gyms/$GYM/program-versions/$2/transition" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "\"$3\""; }

echo "=== setup: owner, head coach, two trainers, two members ==="
OWNER_T=$(signup "ta-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Authority Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

HC_T=$(signup "ta-hc-$S@example.com" "Hana Head");      accept "$HC_T" "$(invite "ta-hc-$S@example.com" '["owner"]')"
T1_T=$(signup "ta-t1-$S@example.com" "Tariq Trainer");  accept "$T1_T" "$(invite "ta-t1-$S@example.com" '["trainer"]')"
T2_T=$(signup "ta-t2-$S@example.com" "Tessa Trainer");  accept "$T2_T" "$(invite "ta-t2-$S@example.com" '["trainer"]')"
M1_T=$(signup "ta-m1-$S@example.com" "Mo Member");      accept "$M1_T" "$(invite "ta-m1-$S@example.com" '["member"]')"
M2_T=$(signup "ta-m2-$S@example.com" "Mia Member");     accept "$M2_T" "$(invite "ta-m2-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); M1=$(uid "$M1_T"); M2=$(uid "$M2_T")

# T1 coaches M1. M2 is deliberately nobody's client.
# Pairing is a two-step handshake (ADR-0034): the owner proposes, the trainer
# accepts. Which is also the setup this section needs — a trainer with a client.
PAIR_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$M1\",\"coach_id\":\"$T1\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$PAIR_REQ/answer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"accept"' >/dev/null

echo ""
echo "=== 1. the catalogue: a trainer may name a movement ==="
ST=$(mkex "$T1_T" "Zercher Squat $S")
check "a trainer may add an exercise" "$ST" "201"
check "  it lands as a proposal" "$(pyq "d['status']")" "proposed"
check "  attributed to whoever raised it" "$(pyq "d['proposed_by']")" "$T1"
PROPOSED=$(pyq "d['id']")

ST=$(mkex "$HC_T" "Pendlay Row $S")
check "a head coach's entry needs no review" "$(pyq "d['status']")" "approved"

ST=$(mkex "$M1_T" "Member Invention $S")
check "a plain member may NOT add exercises" "$ST" "403"

# The point of "proposed, not blocked": the trainer is mid-programme.
ST=$(code "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $T1_T")
check "a proposal is visible in the catalogue immediately" \
  "$(pyq "any(e['id'] == '$PROPOSED' for e in d)")" "True"

echo ""
echo "=== 2. curation is a separate right ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/exercises/$PROPOSED/curate" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '{"decision":"approve"}')
check "a trainer may not approve their own proposal" "$ST" "403"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/exercises/$PROPOSED/curate" -H "authorization: Bearer $T2_T" \
  -H 'content-type: application/json' -d '{"decision":"approve"}')
check "nor another trainer's" "$ST" "403"

ST=$(code "$B/api/v1/gyms/$GYM/exercises/pending" -H "authorization: Bearer $HC_T")
check "a head coach sees the curation queue" "$ST" "200"
check "  the proposal is in it" "$(pyq "any(e['id'] == '$PROPOSED' for e in d)")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/exercises/pending" -H "authorization: Bearer $T1_T")
check "a trainer is refused the queue, not handed an empty one" "$ST" "403"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/exercises/$PROPOSED/curate" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '{"decision":"approve"}')
check "a head coach may approve it" "$ST" "200"
check "  and it is now catalogue vocabulary" "$(pyq "d['status']")" "approved"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/exercises/$PROPOSED/curate" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '{"decision":"approve"}')
check "approving twice is refused" "$ST" "400"

ST=$(code "$B/api/v1/gyms/$GYM/exercises/pending" -H "authorization: Bearer $HC_T")
check "the queue empties as it is worked" "$(pyq "any(e['id'] == '$PROPOSED' for e in d)")" "False"

echo ""
echo "=== 3. the catalogue is the gym's, not a trainer's ==="
# Reversed from ADR-0024 by ADR-0034. The gym decided the catalogue is written
# by the people who run it, and a trainer's job is to APPLY it. What that buys:
# ONE library. Under the old rule five trainers could each write their own
# near-duplicate of "Beginner Strength", and since progress is computed per
# exercise and per version, five variants fragment the very data the product
# exists to accumulate.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Trainer Block $S\",\"focus\":\"strength\"}")
check "a trainer may NOT create a programme" "$ST" "403"

# But they must be able to READ it — it is what they assign from.
ST=$(code "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $T1_T")
check "  though they may read the whole catalogue" "$ST" "200"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Member Block $S\"}")
check "a plain member may not create one either" "$ST" "403"

# The gym writes it.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Gym Block $S\",\"focus\":\"strength\"}")
check "a head coach creates one" "$ST" "201"
V_T1=$(pyq "d['latest_version']['id']")

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V_T1/weeks" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '{"week_number":1}')
check "  and adds content to it" "$ST" "201"
TA_W1=$(pyq "d['id']")

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V_T1/weeks" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '{"week_number":2}')
check "a trainer may NOT add content to it" "$ST" "403"

# The review gate counts PRESCRIBED EXERCISES, not weeks, so give it content.
TA_EX1=$(mkex "$T1_T" "Fill Lift A $S" >/dev/null; pyq "d['id']")
TA_WO1=$(code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$TA_W1/workouts" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Day 1"}' >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$TA_WO1/exercises" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$TA_EX1\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5},\"rir\":2}}" >/dev/null

echo ""
echo "=== 4. publishing: the moves that bind the gym ==="
# The whole lifecycle is a manager's now, so a trainer is refused at the first
# step rather than the third.
ST=$(move "$T1_T" "$V_T1" "submit_for_review")
check "a trainer may NOT send it for review" "$ST" "403"
ST=$(move "$T1_T" "$V_T1" "approve")
check "  nor approve" "$ST" "403"
ST=$(move "$T1_T" "$V_T1" "publish")
check "  nor publish" "$ST" "403"

ST=$(move "$HC_T" "$V_T1" "submit_for_review")
check "the author sends it for review" "$ST" "200"
ST=$(move "$OWNER_T" "$V_T1" "approve")
check "a second person approves it" "$ST" "200"
ST=$(move "$OWNER_T" "$V_T1" "publish")
check "  and publishes it" "$ST" "200"

# The second-person rule is untouched by either ADR, and now that authoring
# and publishing are the same set it is the ONLY thing standing between one
# manager and the whole gym.
code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Self Approve $S\"}" >/dev/null
V_SELF=$(pyq "d['latest_version']['id']")
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V_SELF/weeks" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '{"week_number":1}' >/dev/null
TA_WS=$(pyq "d['id']")
TA_EXS=$(mkex "$HC_T" "Fill Lift Self $S" >/dev/null; pyq "d['id']")
TA_WOS=$(code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$TA_WS/workouts" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Day 1"}' >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$TA_WOS/exercises" -H "authorization: Bearer $HC_T" \
  -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$TA_EXS\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5},\"rir\":2}}" >/dev/null
move "$HC_T" "$V_SELF" "submit_for_review" >/dev/null
ST=$(move "$HC_T" "$V_SELF" "approve")
check "nobody approves their own version, head coach included" "$ST" "400"

ST=$(move "$OWNER_T" "$V_SELF" "approve")
check "  a second person can" "$ST" "200"

echo "=== 5. assignment: your own clients, and only those ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M1\",\"program_version_id\":\"$V_T1\",\"start_date\":\"$TODAY\"}")
check "a trainer may assign to their own client" "$ST" "201"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$M2\",\"program_version_id\":\"$V_T1\",\"start_date\":\"$TODAY\"}")
check "  and to nobody else" "$ST" "403"

# The client bug this suite exists to stop coming back: the assign screen used
# to read the roster, which is head-coach-only, so a trainer saw an empty list.
# The relationship list is what it must read instead — assert that it carries
# the name the picker renders.
ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $T1_T")
check "a trainer's client list names the athlete" "$(pyq "d[0]['athlete_name']")" "Mo Member"
ST=$(code "$B/api/v1/gyms/$GYM/members" -H "authorization: Bearer $T1_T")
check "  while the roster stays closed to them" "$ST" "403"

echo ""
echo "=== 6. the audit trail records curation ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
check "audit readable by owner" "$ST" "200"
check "  exercise.approved was recorded" \
  "$(pyq "any(e['action'] == 'exercise.approved' for e in d)")" "True"
check "  with the curator as the actor, not the proposer" \
  "$(pyq "next(e['actor_name'] for e in d if e['action'] == 'exercise.approved')")" "Hana Head"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
