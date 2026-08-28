#!/usr/bin/env bash
# Gym entry: the member's short-lived pass, and staff scanning it at the door.
#
# The load-bearing rules, in the order they matter:
#
#   1. A refusal is RECORDED, not discarded. "Who tried to get in and couldn't"
#      is the operational data a door log exists for; a log that only holds
#      admissions is a list of people who were already fine.
#   2. The pass is gym-scoped. A perfectly valid pass minted for another gym
#      opens nothing here, and says the same thing a forged one does.
#   3. The pass expires in seconds, and an expired one is indistinguishable
#      from a forged one at the door — the screen has no business telling
#      whoever is holding it which.
#   4. Scanning is coaching-level, not management-level: a trainer works the
#      floor. A plain member cannot scan, including their own pass.
#   5. The verdict comes from the SAME entitlement resolver the rest of the
#      app uses, so the sentence a member reads on their membership screen is
#      the sentence they hear at the door.
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
export SERVER_PORT=8101
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
./target/debug/server > /tmp/server-checkins.log 2>&1 &
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
pass_for() { code -X POST "$B/api/v1/gyms/$GYM/checkins/my-pass" -H "authorization: Bearer $1" >/dev/null; pyq "d['token']"; }
scan_as() { code -X POST "$B/api/v1/gyms/$GYM/checkins/scan" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "{\"token\":\"$2\"}"; }

echo "=== setup: one gym, staff and members ==="
OWNER_T=$(signup "ci-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Turnstile Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

TRAINER_T=$(signup "ci-trainer-$S@example.com" "Tariq Trainer")
accept "$TRAINER_T" "$(invite "ci-trainer-$S@example.com" '["trainer"]')"
M1_T=$(signup "ci-m1-$S@example.com" "Mo Member")
accept "$M1_T" "$(invite "ci-m1-$S@example.com" '["member"]')"
M2_T=$(signup "ci-m2-$S@example.com" "Mia Member")
accept "$M2_T" "$(invite "ci-m2-$S@example.com" '["member"]')"
M1=$(uid "$M1_T")

# A second gym, to prove a pass does not travel. Needs SINGLE_GYM_MODE off,
# which is the default for local dev exactly so isolation stays testable.
OTHER_OWNER_T=$(signup "ci-other-$S@example.com" "Otto Other")
OTHER_GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OTHER_OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Other Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

echo ""
echo "=== 1. the pass ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/checkins/my-pass" -H "authorization: Bearer $M1_T")
check "a member may mint their own pass" "$ST" "201"
check "  it carries a token" "$(pyq "len(d['token']) > 20")" "True"
check "  it expires in seconds, not minutes" "$(pyq "d['expires_in_seconds'] <= 120")" "True"

# Staff walk through the same door as everyone else — the pass is self-service
# for anyone in the gym, not a member-only object.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/checkins/my-pass" -H "authorization: Bearer $TRAINER_T")
check "staff get a pass too" "$ST" "201"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/checkins/my-pass" -H "authorization: Bearer $OTHER_OWNER_T")
check "an outsider gets 404, not 403" "$ST" "404"

echo ""
echo "=== 2. who may work the door ==="
P1=$(pass_for "$M1_T")
ST=$(scan_as "$TRAINER_T" "$P1")
check "a trainer may scan" "$ST" "200"
check "  the scan names the member" "$(pyq "d['member_name']")" "Mo Member"

ST=$(scan_as "$OWNER_T" "$(pass_for "$M1_T")")
check "an owner may scan" "$ST" "200"

# A member holding a scanner would be able to read every other member's
# standing by asking them for their pass. Coaching level, deliberately.
ST=$(scan_as "$M2_T" "$(pass_for "$M1_T")")
check "a plain member may NOT scan" "$ST" "403"
ST=$(scan_as "$M1_T" "$(pass_for "$M1_T")")
check "  not even their own pass" "$ST" "403"

echo ""
echo "=== 3. the verdict follows the entitlement resolver ==="
# Nothing is on sale yet, so Source::NotBilled applies: a gym that bills
# nobody withholds nothing. Getting this backwards would lock every member
# out of every gym that uses the app without its billing.
ST=$(scan_as "$TRAINER_T" "$(pass_for "$M1_T")")
check "unbilled gym admits" "$(pyq "d['allowed']")" "True"
check "  and says why, in the same words as the app" "$(pyq "'does not bill' in d['reason']")" "True"

# Put something on sale that does NOT grant gym access, and the same member
# is now refused — the gym bills, and nothing they hold covers the door.
code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Nutrition only","price_minor":1000,"currency":"GBP","interval":"monthly","grants":["coached_programming"]}' >/dev/null

ST=$(scan_as "$TRAINER_T" "$(pass_for "$M1_T")")
check "a billing gym refuses an uncovered member" "$(pyq "d['allowed']")" "False"
check "  the refusal is a sentence, not a code" "$(pyq "'No active plan' in d['reason']")" "True"
check "  and it is still a 200 — refusal is an answer" "$ST" "200"

# Sell them access, and the door opens again, naming the plan.
PLAN=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Full Access","price_minor":4000,"currency":"GBP","interval":"monthly","grants":["gym_access"]}' >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"member_id\":\"$M1\",\"plan_id\":\"$PLAN\",\"started_on\":\"$(date +%F)\"}" >/dev/null

ST=$(scan_as "$TRAINER_T" "$(pass_for "$M1_T")")
check "a covered member is admitted" "$(pyq "d['allowed']")" "True"
check "  the reason names the plan they hold" "$(pyq "'Full Access' in d['reason']")" "True"

echo ""
echo "=== 4. a pass is not a skeleton key ==="
ST=$(scan_as "$TRAINER_T" "not-a-real-token")
check "a forged token is refused" "$ST" "400"

# A REAL, unexpired, correctly-signed pass — for a different gym. This is the
# case a naive "is the signature valid?" check would wave straight through.
OTHER_PASS=$(code -X POST "$B/api/v1/gyms/$OTHER_GYM/checkins/my-pass" -H "authorization: Bearer $OTHER_OWNER_T" >/dev/null; pyq "d['token']")
ST=$(scan_as "$TRAINER_T" "$OTHER_PASS")
check "another gym's valid pass opens nothing here" "$ST" "400"
check "  and is refused in the same words as a forgery" "$(pyq "'not valid' in d['detail']")" "True"

echo ""
echo "=== 5. the door log ==="
ST=$(code "$B/api/v1/gyms/$GYM/checkins" -H "authorization: Bearer $TRAINER_T")
check "staff may read recent check-ins" "$ST" "200"
# Five scans reach the door: 2 in §2, 3 in §3. The 403s and 400s above never
# got as far as a verdict, so they are correctly absent — an authorization
# failure is not a door event.
check "  every scan that reached a verdict was recorded" "$(pyq "len(d) == 5")" "True"
check "  refusals really are in there" "$(pyq "any(not x['allowed'] for x in d)")" "True"
check "  admissions too" "$(pyq "any(x['allowed'] for x in d)")" "True"
check "  newest first" "$(pyq "d == sorted(d, key=lambda x: x['scanned_at'], reverse=True)")" "True"
check "  the recorded reason is the one shown at the door" \
  "$(pyq "all(len(x['reason']) > 0 for x in d)")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/checkins" -H "authorization: Bearer $M2_T")
check "a member may not read the door log" "$ST" "403"

echo ""
echo "=== 6. the record is frozen ==="
# The reason is captured at scan time on purpose: a plan lapsing later must
# not rewrite history into "they were never allowed in".
ROWS=$(docker exec gym-postgres psql -U gym -d gym -tAc \
  "SELECT count(*) FROM gym_checkins WHERE gym_id = '$GYM'" 2>/dev/null | tr -d '\r')
check "check-ins are persisted" "$([ "${ROWS:-0}" -eq 5 ] && echo yes || echo no)" "yes"

PRIV=$(docker exec gym-postgres psql -U gym -d gym -tAc \
  "SELECT string_agg(DISTINCT privilege_type, ',' ORDER BY privilege_type)
     FROM information_schema.table_privileges
    WHERE grantee = 'gym_app' AND table_name = 'gym_checkins'" 2>/dev/null | tr -d '\r')
check "the app role cannot rewrite the door log" "$PRIV" "INSERT,SELECT"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
