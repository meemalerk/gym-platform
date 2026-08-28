#!/usr/bin/env bash
# Entitlements, end to end over HTTP.
#
# The rule this exists to prove is the one that would break every gym on day
# one if it were backwards: **a gym that has not set up plans is not a gym
# whose members may not train.** Absence of billing is absence of billing, not
# absence of entitlement.
#
# After that, the interesting cases are all about not locking the wrong person
# out: an owner behind on their own gym's bill, a member whose plan was
# archived under them, and a member whose subscription was never active.
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
export SERVER_PORT=8095
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
./target/debug/server > /tmp/server-entitlements.log 2>&1 &
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

# The features held, as a sorted comma list — order-independent comparison.
feats() { pyq "','.join(sorted(e['feature'] for e in d['held']))"; }
# Where one feature came from, by name.
srcof() { pyq "next((e['source']['kind'] for e in d['held'] if e['feature']=='$1'), 'none')"; }
whyof() { pyq "next((e['because'] for e in d['held'] if e['feature']=='$1'), 'none')"; }

S=$(date +%s%N); PW="correct horse battery staple"; TODAY=$(date +%F)
NOW=$(date -u +%Y-%m-%dT%H:%M:%SZ)
LAST_MONTH=$(date -d "-1 month" +%F)

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

echo "=== setup: a gym, a published programme, three members, no plans yet ==="
OWNER_T=$(signup "en-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Entitlement Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)
HC_T=$(signup "en-hc-$S@example.com" "Hana Head");   accept "$HC_T" "$(invite "en-hc-$S@example.com" '["owner"]')"
T1_T=$(signup "en-t1-$S@example.com" "Tariq Trainer"); accept "$T1_T" "$(invite "en-t1-$S@example.com" '["trainer"]')"
PAID_T=$(signup "en-paid-$S@example.com" "Pia Paid");  accept "$PAID_T" "$(invite "en-paid-$S@example.com" '["member"]')"
FREE_T=$(signup "en-free-$S@example.com" "Fred Free"); accept "$FREE_T" "$(invite "en-free-$S@example.com" '["member"]')"
LAPSED_T=$(signup "en-laps-$S@example.com" "Lena Lapsed"); accept "$LAPSED_T" "$(invite "en-laps-$S@example.com" '["member"]')"
OUTSIDER_T=$(signup "en-out-$S@example.com" "Otto Outsider")
T1=$(uid "$T1_T"); PAID=$(uid "$PAID_T"); FREE=$(uid "$FREE_T"); LAPSED=$(uid "$LAPSED_T")

for M in "$PAID" "$FREE" "$LAPSED"; do
  # Pairing is a two-step handshake (ADR-0034): the manager proposes, the named
  # trainer accepts. Direct pairing is gone — the relationship hands the trainer
  # that member's whole training history, so they get asked first.
  PAIR_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $HC_T" \
    -H 'content-type: application/json' -d "{\"athlete_id\":\"$M\",\"coach_id\":\"$T1\"}" >/dev/null; pyq "d['id']")
  code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$PAIR_REQ/answer" -H "authorization: Bearer $T1_T" \
    -H 'content-type: application/json' -d '"accept"' >/dev/null
done

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
for STEP in submit_for_review approve publish; do
  TOK=$OWNER_T; [ "$STEP" = "submit_for_review" ] || TOK=$HC_T
  code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V/transition" -H "authorization: Bearer $TOK" \
    -H 'content-type: application/json' -d "\"$STEP\"" >/dev/null
done
assign() {
  code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $T1_T" \
    -H 'content-type: application/json' \
    -d "{\"athlete_id\":\"$1\",\"program_version_id\":\"$V\",\"start_date\":\"$TODAY\"}" >/dev/null
  pyq "d['id']"
}
A_PAID=$(assign "$PAID"); A_FREE=$(assign "$FREE"); A_LAPSED=$(assign "$LAPSED")

start() { # start <token> <assignment> -> http status
  code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' \
    -d "{\"id\":\"$(newid)\",\"assignment_id\":\"$2\",\"workout_template_id\":\"$WORKOUT\",\"started_at\":\"$NOW\"}"
}

echo ""
echo "=== 1. a gym that sells nothing withholds nothing ==="
ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $FREE_T")
check "a member can ask what they hold -> 200" "$ST" "200"
check "  and holds everything" "$(feats)" "class_credits,coached_programming,gym_access"
check "  sourced 'not billed', not a silent default" "$(srcof gym_access)" "not_billed"
check "  which the screen can print verbatim" "$(whyof gym_access)" "This gym does not bill through the app"

ST=$(start "$FREE_T" "$A_FREE")
check "so an unbilled member trains -> 201" "$ST" "201"

ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $OUTSIDER_T")
check "an outsider gets 404, never a feature list" "$ST" "404"

ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me")
check "and an anonymous caller gets 401" "$ST" "401"

echo ""
echo "=== 2. the moment a gym offers a plan, the gate is live ==="
mkplan() { # mkplan <name> <price> <grants-json> -> plan id
  code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
    -H 'content-type: application/json' \
    -d "{\"name\":\"$1\",\"price_minor\":$2,\"currency\":\"GBP\",\"interval\":\"monthly\",\"grants\":$3}" >/dev/null
  pyq "d['id']"
}
OPEN_GYM=$(mkplan "Open Gym" 4900 '["gym_access"]')
COACHING=$(mkplan "Coaching" 12000 '["gym_access","coached_programming"]')

ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $FREE_T")
check "the same member now holds nothing" "$(feats)" ""
check "  and is told so rather than 500ing" "$ST" "200"

ST=$(start "$FREE_T" "$A_FREE")
check "an unsubscribed member cannot start -> 403" "$ST" "403"

echo ""
echo "=== 3. what you bought is what you hold ==="
code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$PAID\",\"plan_id\":\"$COACHING\",\"started_on\":\"$LAST_MONTH\"}" >/dev/null
ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $PAID_T")
check "Coaching confers exactly what it lists" "$(feats)" "coached_programming,gym_access"
check "  named by the plan, not by the rule" "$(whyof coached_programming)" "Your Coaching membership"
check "  and does NOT confer what it does not list" "$(pyq "any(e['feature']=='class_credits' for e in d['held'])")" "False"

ST=$(start "$PAID_T" "$A_PAID")
check "a subscribed member trains -> 201" "$ST" "201"

code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$LAPSED\",\"plan_id\":\"$OPEN_GYM\",\"started_on\":\"$LAST_MONTH\"}" >/dev/null
ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $LAPSED_T")
check "Open Gym is access only" "$(feats)" "gym_access"
check "  sourced from the subscription" "$(srcof gym_access)" "subscription"

echo ""
echo "=== 4. an unpaid invoice does not lock anyone out (yet) ==="
# Deliberate: dunning is not built. Access follows the SUBSCRIPTION's state, and
# nothing moves that state on a missed payment — so an overdue member still
# trains. Pinned so the day suspension lands, this test changes on purpose
# rather than a member quietly losing the gym.
ST=$(code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $LAPSED_T")
check "the member's own invoice is visible -> 200" "$ST" "200"
check "  and it is unpaid" "$(pyq "d[0]['status']['state']")" "due"
ST=$(start "$LAPSED_T" "$A_LAPSED")
check "an owing member still trains -> 201 (no dunning yet)" "$ST" "201"

echo ""
echo "=== 5. archiving a plan does not evict its subscribers ==="
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/plans/$OPEN_GYM" -H "authorization: Bearer $OWNER_T")
check "owner archives Open Gym -> 204" "$ST" "204"
ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $LAPSED_T")
check "the existing subscriber keeps access" "$(feats)" "gym_access"
check "  still named after the plan they bought" "$(whyof gym_access)" "Your Open Gym membership"

# And with EVERY plan archived the gym stops billing entirely — the rule from
# §1 again, reached from the other direction.
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/plans/$COACHING" -H "authorization: Bearer $OWNER_T")
check "owner archives the last plan -> 204" "$ST" "204"
ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $FREE_T")
check "a gym with nothing on sale bills nobody again" "$(feats)" "class_credits,coached_programming,gym_access"
check "  sourced 'not billed'" "$(srcof gym_access)" "not_billed"
ST=$(start "$FREE_T" "$A_FREE")
check "and the unsubscribed member trains again -> 201" "$ST" "201"

echo ""
echo "=== 6. managers are never locked out of their own gym ==="
# An owner whose gym sells plans they have not bought must still be able to get
# in and fix the billing — the failure mode where nobody can unlock anything.
RELIST=$(mkplan "Members only" 4900 '["gym_access"]')
check "a plan is on sale again" "$(pyq "d['is_offered']")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $OWNER_T")
check "the owner holds no membership either" "$(feats)" ""
OWNER_ID=$(uid "$OWNER_T")
# Assigned by the owner themselves — a manager may assign to anyone, which is
# the path a solo user takes in their own gym.
code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$OWNER_ID\",\"program_version_id\":\"$V\",\"start_date\":\"$TODAY\"}" >/dev/null
A_OWNER=$(pyq "d['id']")
ST=$(start "$OWNER_T" "$A_OWNER")
check "  but is not gated out of their own floor -> 201" "$ST" "201"

# A head coach is NOT a manager for this purpose: they run programming, not the
# money, and gating them would be the same lockout in a different seat — so the
# check is that the rule is deliberately scoped, not accidentally generous.
ST=$(code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $HC_T")
check "a head coach's own entitlements are read normally" "$ST" "200"
check "  and they hold none without a membership" "$(feats)" ""

echo ""
echo "=== 7. plans state what they grant ==="
ST=$(code "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $FREE_T")
check "a member reads the price list -> 200" "$ST" "200"
check "  including what each plan unlocks" \
  "$(pyq "','.join(sorted(p['grants'][0] for p in d if p['name']=='Members only'))")" "gym_access"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Hollow","price_minor":900,"currency":"GBP","interval":"monthly","grants":[]}')
check "a plan that grants nothing is refused -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Teleport","price_minor":900,"currency":"GBP","interval":"monthly","grants":["teleportation"]}')
check "an unknown feature is refused at the edge -> 422" "$ST" "422"

echo ""
echo "  PASSED: $PASS   FAILED: $FAIL"
[ "$FAIL" -eq 0 ] || { echo "--- server log ---"; tail -20 /tmp/server-entitlements.log; }
exit $((FAIL > 0))
