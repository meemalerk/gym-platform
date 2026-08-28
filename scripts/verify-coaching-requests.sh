#!/usr/bin/env bash
# Choosing a coach, and the trainer directory (ADR-0025, amended by ADR-0031).
#
# The two-sided handshake is gone: a member picks a coach and is coached by
# them from that moment. What is tested here is everything that guarded the
# edges of that flow and still has to hold — the directory is not the roster,
# the endpoint is not a membership oracle, the pairing and its record land
# together, and the coach can end it.
#
# The claim: a coaching relationship is an ACCESS GRANT, so it takes consent
# from both sides. A member asks; the coach answers. Neither party can create
# the grant alone.
#
# The two things most worth pinning:
#
#   1. The directory is NOT the roster. It is open to members — that is the
#      whole point — so it must carry no email and no other member's name, or
#      opening it would quietly undo `GymService::roster`'s privacy rule.
#   2. Accepting creates the pairing IN THE SAME TRANSACTION. A window where a
#      member has been told yes and their coach cannot see them would be
#      invisible in testing and infuriating in use.
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
export SERVER_PORT=8103
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
./target/debug/server > /tmp/server-requests.log 2>&1 &
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
# Invitations are gone (ADR-0031): people join through the open door and a
# manager sets their standing. `invite` now just carries the capacities
# through and `accept_inv` joins-then-promotes, so the call sites below read
# exactly as they did.
invite() { printf '%s' "$2"; }
accept_inv() {
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
ask() { code -X POST "$B/api/v1/gyms/$GYM/coaching-requests" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "{\"coach_id\":\"$2\",\"message\":${3:-null}}"; }
answer() { code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$2/answer" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "\"$3\""; }

echo "=== setup: owner, head coach, two trainers, three members ==="
OWNER_T=$(signup "cr-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Matching Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

HC_T=$(signup "cr-hc-$S@example.com" "Hana Head");     accept_inv "$HC_T" "$(invite "cr-hc-$S@example.com" '["owner"]')"
T1_T=$(signup "cr-t1-$S@example.com" "Tariq Trainer"); accept_inv "$T1_T" "$(invite "cr-t1-$S@example.com" '["trainer"]')"
T2_T=$(signup "cr-t2-$S@example.com" "Tessa Trainer"); accept_inv "$T2_T" "$(invite "cr-t2-$S@example.com" '["trainer"]')"
# Deliberately never publishes a trainer profile: the directory LEFT JOINs it,
# and a coach who has not filled one in must still be browsable with empty
# lists rather than nulls. That case used to be carried by the head coach.
T3_T=$(signup "cr-t3-$S@example.com" "Bare Trainer");  accept_inv "$T3_T" "$(invite "cr-t3-$S@example.com" '["trainer"]')"
M1_T=$(signup "cr-m1-$S@example.com" "Mo Member");     accept_inv "$M1_T" "$(invite "cr-m1-$S@example.com" '["member"]')"
M2_T=$(signup "cr-m2-$S@example.com" "Mia Member");    accept_inv "$M2_T" "$(invite "cr-m2-$S@example.com" '["member"]')"
M3_T=$(signup "cr-m3-$S@example.com" "Max Member");    accept_inv "$M3_T" "$(invite "cr-m3-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); T2=$(uid "$T2_T"); M1=$(uid "$M1_T"); M2=$(uid "$M2_T")

# Coaches publish professional profiles — that is what the directory shows.
code -X PUT "$B/api/v1/me/profiles/trainer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' \
  -d '{"headline":"Strength and powerlifting","specialties":["Powerlifting","Beginners"],"certifications":["L3 PT"]}' >/dev/null
code -X PUT "$B/api/v1/me/profiles/trainer" -H "authorization: Bearer $T2_T" \
  -H 'content-type: application/json' \
  -d '{"headline":"Mobility and rehab","specialties":["Prenatal yoga"]}' >/dev/null

echo ""
echo "=== 1. the directory is open, and is not the roster ==="
ST=$(code "$B/api/v1/gyms/$GYM/trainers" -H "authorization: Bearer $M1_T")
check "a member may browse coaches" "$ST" "200"
check "  all three trainers appear" "$(pyq "len(d)")" "3"
check "  with the headline they published" \
  "$(pyq "next(x['headline'] for x in d if x['display_name'] == 'Tariq Trainer')")" "Strength and powerlifting"
check "  and their specialties" \
  "$(pyq "'Powerlifting' in next(x['specialties'] for x in d if x['display_name'] == 'Tariq Trainer')")" "True"
check "  a coach with no profile still appears" \
  "$(pyq "any(x['display_name'] == 'Bare Trainer' for x in d)")" "True"
check "  with empty lists, not nulls" \
  "$(pyq "next(x['specialties'] for x in d if x['display_name'] == 'Bare Trainer') == []")" "True"
# The directory is coaches, not staff. Padding "choose your coach" with
# whoever holds the keys is not a directory — and an owner who genuinely
# coaches holds `trainer` too, so they appear on that basis (ADR-0014).
check "  the owner is NOT in it"   "$(pyq "any(x['display_name'] == 'Hana Head' for x in d)")" "False"
check "  NO email is exposed" "$(pyq "any('email' in x for x in d)")" "False"
check "  and no member's name leaks into it" \
  "$(pyq "any('Member' in x['display_name'] for x in d)")" "False"
check "  client counts start at zero" \
  "$(pyq "next(x['active_clients'] for x in d if x['display_name'] == 'Tariq Trainer')")" "0"

# The roster remains closed. If this ever flips, the directory stopped being a
# narrower thing and became a leak.
ST=$(code "$B/api/v1/gyms/$GYM/members" -H "authorization: Bearer $M1_T")
check "the roster is still closed to members" "$ST" "403"

echo ""
echo "=== 2. choosing a coach ==="
ST=$(ask "$M1_T" "$T1" '"Back to squatting after an ankle injury"')
check "a member may choose a coach" "$ST" "201"
# The handshake is gone (ADR-0031). Nothing is pending, ever.
check "  and is coached from that moment" "$(pyq "d['status']['state']")" "accepted"
check "  the note is kept" "$(pyq "'ankle' in d['message']")" "True"
REQ1=$(pyq "d['id']")

# The pairing is the point. It exists before the coach has done anything.
ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $T1_T")
check "the coach can see them straight away" \
  "$(pyq "any(r['athlete_id'] == '$M1' and r['is_active'] for r in d)")" "True"
check "  attributed to the athlete, who is the one who decided" \
  "$(pyq "next(r for r in d if r['athlete_id'] == '$M1')['coach_id'] == '$T1'")" "True"

ST=$(ask "$M1_T" "$T1")
check "choosing the same coach twice is refused" "$ST" "409"

ST=$(ask "$M1_T" "$T2")
check "  but a second coach is fine" "$ST" "201"
REQ2=$(pyq "d['id']")

# 404, not 400: "in this gym but not a coach" must be indistinguishable from
# "not in this gym", or the endpoint becomes a membership oracle you can probe
# with a user id.
ST=$(ask "$M1_T" "$M2")
check "choosing a non-coach is refused without confirming they exist" "$ST" "404"

# Asked by a COACH, deliberately. A member pointing at themselves is already
# caught one line earlier by "you are not a coach here" (404), so the domain's
# self-request rule is only reachable by someone who genuinely could coach —
# a trainer who also trains here and got confused about which list they were
# looking at. That is the case worth pinning, and it leaks nothing, so it gets
# a plain useful 400 rather than an evasive 404.
ST=$(ask "$T1_T" "$T1")
check "a coach cannot choose themselves" "$ST" "400"

MY_ID=$(uid "$M1_T")
ST=$(ask "$M1_T" "$MY_ID")
check "a member pointing at themselves is just 'not a coach'" "$ST" "404"

echo ""
echo "=== 3. who can see what ==="
ST=$(code "$B/api/v1/gyms/$GYM/coaching-requests" -H "authorization: Bearer $T1_T")
check "a coach sees the choices addressed to them" "$(pyq "len(d)")" "1"
check "  named, so the screen needs no second call" "$(pyq "d[0]['athlete_name']")" "Mo Member"

ST=$(code "$B/api/v1/gyms/$GYM/coaching-requests" -H "authorization: Bearer $T2_T")
check "another coach sees only their own" "$(pyq "len(d)")" "1"
check "  and it is a different one" "$(pyq "d[0]['id'] == '$REQ2'")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/coaching-requests" -H "authorization: Bearer $M1_T")
check "the member sees both of theirs" "$(pyq "len(d)")" "2"

ST=$(code "$B/api/v1/gyms/$GYM/coaching-requests" -H "authorization: Bearer $M2_T")
check "an uninvolved member sees none" "$(pyq "len(d)")" "0"

ST=$(code "$B/api/v1/gyms/$GYM/coaching-requests" -H "authorization: Bearer $HC_T")
check "a manager sees the gym's" "$(pyq "len(d)")" "2"

echo ""
echo "=== 4. nothing is left to answer ==="
# `answer` and `withdraw` survive on the server for rows raised before
# ADR-0031, and neither client offers them. A record that is already resolved
# refuses both — which is what every record is now.
ST=$(answer "$T1_T" "$REQ1" "decline")
check "an already-resolved choice cannot be answered" "$ST" "409"
ST=$(answer "$HC_T" "$REQ2" "accept")
check "  not even by a manager" "$ST" "409"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$REQ1/withdraw" -H "authorization: Bearer $M1_T")
check "  and it cannot be withdrawn after the fact" "$ST" "409"

echo ""
echo "=== 5. the coach is not trapped ==="
# The reason it is safe to pair without asking the coach: ending is one call,
# and it is theirs to make. (Assigning a client still is not — that would be a
# self-service grant of access to somebody's data. Ending is the opposite.)
REL=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $HC_T" >/dev/null
      pyq "next(r['id'] for r in d if r['athlete_id'] == '$M1' and r['coach_id'] == '$T2')")
ST=$(code -X POST "$B/api/v1/gyms/$GYM/coach-relationships/$REL/end" -H "authorization: Bearer $HC_T")
check "a manager may end a pairing" "$ST" "200"
check "  and it is no longer active" "$(pyq "d['is_active']")" "False"

# Ended, so the member may choose them again — circumstances change.
ST=$(ask "$M1_T" "$T2")
check "the member may choose them again afterwards" "$ST" "201"

echo ""
echo "=== 6. the record survives ==="
ST=$(code "$B/api/v1/gyms/$GYM/trainers" -H "authorization: Bearer $M1_T")
check "the directory shows the new client count" \
  "$(pyq "next(x['active_clients'] for x in d if x['display_name'] == 'Tariq Trainer')")" "1"

ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
check "the pairing was audited" \
  "$(pyq "any(e['action'] == 'coach_relationship.created' for e in d)")" "True"
check "  and says the athlete chose it" \
  "$(pyq "any(e['action'] == 'coach_relationship.created' and (e.get('metadata') or {}).get('reason') == 'chosen_by_athlete' for e in d)")" "True"

PRIV=$(docker exec gym-postgres psql -U gym -d gym -tAc \
  "SELECT string_agg(DISTINCT privilege_type, ',' ORDER BY privilege_type)
     FROM information_schema.table_privileges
    WHERE grantee = 'gym_app' AND table_name = 'coaching_requests'" 2>/dev/null | tr -d '\r')
check "requests are resolved, never deleted" "$PRIV" "INSERT,SELECT,UPDATE"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
