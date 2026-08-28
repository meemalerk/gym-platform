#!/usr/bin/env bash
# Coach–athlete relationships, end to end over HTTP.
#
# The point of this feature is a permission boundary, so that is what this tests
# hardest: a trainer who coaches SOMEONE must not thereby see EVERYONE. The
# domain tests prove the rule in isolation; this proves the API actually applies
# it, which is a different claim.
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
./target/debug/server > /tmp/server-coaching.log 2>&1 &
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

# A gym with a head coach, two trainers and two members — the smallest cast that
# can demonstrate "sees own clients, not everyone".
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
OWNER_T=$(signup "co-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Coaching Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

HEAD_T=$(signup "co-head-$S@example.com" "Hana Head");     accept "$HEAD_T"  "$(invite "co-head-$S@example.com" '["owner"]')"
T1_T=$(signup "co-t1-$S@example.com" "Tariq Trainer");     accept "$T1_T"    "$(invite "co-t1-$S@example.com" '["trainer"]')"
T2_T=$(signup "co-t2-$S@example.com" "Tessa Trainer");     accept "$T2_T"    "$(invite "co-t2-$S@example.com" '["trainer"]')"
M1_T=$(signup "co-m1-$S@example.com" "Mo Member");         accept "$M1_T"    "$(invite "co-m1-$S@example.com" '["member"]')"
M2_T=$(signup "co-m2-$S@example.com" "Mia Member");        accept "$M2_T"    "$(invite "co-m2-$S@example.com" '["member"]')"

T1=$(uid "$T1_T"); T2=$(uid "$T2_T"); M1=$(uid "$M1_T"); M2=$(uid "$M2_T"); HEAD=$(uid "$HEAD_T")

echo ""
# Helpers for the two-step pairing (ADR-0034). There is no direct-pairing
# endpoint any more: a manager PROPOSES and the named trainer ACCEPTS, because
# the relationship hands that trainer the member's whole training history.
propose() { # propose <token> <coach_id> <athlete_id>
  code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$3\",\"coach_id\":\"$2\"}"
}
answer() { # answer <token> <request_id> <accept|decline>
  code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$2/answer" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' -d "\"$3\""
}

echo "=== 1. pairing takes the trainer's consent ==="
ST=$(propose "$HEAD_T" "$T1" "$M1")
check "head coach proposes -> 201" "$ST" "201"
REQ1=$(pyq "d['id']")
check "  it is pending, not a pairing yet" "$(pyq "d['is_pending']")" "True"
check "  and marked as the gym's proposal" "$(pyq "d['is_proposal']")" "True"

# The rule the whole step exists for: the proposer cannot wave it through.
ST=$(answer "$HEAD_T" "$REQ1" accept)
check "the proposer may NOT accept their own proposal -> 403" "$ST" "403"
ST=$(answer "$OWNER_T" "$REQ1" accept)
check "  nor any other manager -> 403" "$ST" "403"
ST=$(answer "$T2_T" "$REQ1" accept)
check "  nor a different trainer -> 403" "$ST" "403"

ST=$(answer "$T1_T" "$REQ1" accept)
check "the named trainer accepts -> 200" "$ST" "200"

ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $HEAD_T")
check "  and the pairing now exists" "$(pyq "any(r['coach_id']=='$T1' and r['athlete_id']=='$M1' and r['is_active'] for r in d)")" "True"
REL1=$(pyq "next(r['id'] for r in d if r['coach_id']=='$T1' and r['athlete_id']=='$M1')")

ST=$(propose "$HEAD_T" "$T1" "$M1")
check "proposing a pair who already work together -> 409" "$ST" "409"

# The escalation these rules exist to prevent: a trainer acquiring a client,
# and the data access that comes with one, on their own say-so.
ST=$(propose "$T1_T" "$T1" "$M2")
check "a trainer may NOT propose themselves a client -> 403" "$ST" "403"
ST=$(propose "$M1_T" "$T1" "$M2")
check "a member may NOT propose -> 403" "$ST" "403"

# A member cannot coach. Caught by capacities, not by job title — and reported
# as NOT FOUND rather than "unsuitable", matching the member-initiated path:
# a specific refusal here would turn the endpoint into a membership oracle.
ST=$(propose "$HEAD_T" "$M2" "$M1")
check "a member named as coach -> 404" "$ST" "404"

OUT_T=$(signup "co-out-$S@example.com" "Ozzy Outsider"); OUT=$(uid "$OUT_T")
ST=$(propose "$HEAD_T" "$T1" "$OUT")
check "an athlete from outside the gym -> 404" "$ST" "404"

# A trainer who does not want the client declines, and no pairing is made.
ST=$(propose "$HEAD_T" "$T2" "$M1")
check "a second proposal for the same member -> 201" "$ST" "201"
REQ_DECLINE=$(pyq "d['id']")
ST=$(answer "$T2_T" "$REQ_DECLINE" decline)
check "  the trainer declines -> 200" "$ST" "200"
ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $HEAD_T")
check "  and no pairing was made" "$(pyq "any(r['coach_id']=='$T2' and r['athlete_id']=='$M1' for r in d)")" "False"

# Second real pairing, so "sees only their own" below is a real distinction.
ST=$(propose "$HEAD_T" "$T2" "$M2")
check "second pairing proposed -> 201" "$ST" "201"
REQ2=$(pyq "d['id']")
ST=$(answer "$T2_T" "$REQ2" accept)
check "  and accepted -> 200" "$ST" "200"

echo "=== 2. visibility — the whole point ==="
ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $HEAD_T")
check "head coach sees the roster -> 200" "$ST" "200"
check "  sees both pairings" "$(pyq "len(d)")" "2"
check "  names resolved for display" "$(pyq "any(r['coach_name']=='Tariq Trainer' for r in d)")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $T1_T")
check "trainer lists -> 200" "$ST" "200"
check "  sees exactly their own client" "$(pyq "len(d)")" "1"
check "  which is the right one" "$(pyq "d[0]['athlete_id'] == '$M1'")" "True"
# Being able to coach someone must never imply seeing everyone.
check "  does NOT see the other trainer's client" "$(pyq "any(r['athlete_id']=='$M2' for r in d)")" "False"

ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $M1_T")
check "member sees their own coach" "$(pyq "len(d)")" "1"
check "  which is their coach" "$(pyq "d[0]['coach_id'] == '$T1'")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $M2_T")
check "the other member sees only theirs" "$(pyq "d[0]['coach_id'] == '$T2'")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $OUT_T")
check "outsider -> 404 (not 403)" "$ST" "404"

echo ""
echo "=== 3. ending ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/coach-relationships/$REL1/end" -H "authorization: Bearer $T1_T")
check "trainer ends their own relationship -> 403" "$ST" "403"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/coach-relationships/$REL1/end" -H "authorization: Bearer $HEAD_T")
check "head coach ends -> 200" "$ST" "200"
check "  no longer active" "$(pyq "d['is_active']")" "False"
check "  records who ended it" "$(pyq "d['status']['ended_by'] == '$HEAD'")" "True"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/coach-relationships/$REL1/end" -H "authorization: Bearer $HEAD_T")
check "ending twice -> 409" "$ST" "409"

# Access must stop when the relationship does.
ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $T1_T")
check "ex-coach sees no active client" "$(pyq "sum(1 for r in d if r['is_active'])")" "0"
# ...but the record survives, or past work loses its author.
check "  the ended record is still visible to them" "$(pyq "len(d)")" "1"

ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $HEAD_T")
check "head coach still sees both records" "$(pyq "len(d)")" "2"
check "  one active, one ended" "$(pyq "sum(1 for r in d if r['is_active'])")" "1"

# Re-pairing after ending must work — the unique index is partial on active
# only — and it goes through consent again, because it is a fresh grant.
ST=$(propose "$HEAD_T" "$T1" "$M1")
check "re-proposing after ending -> 201" "$ST" "201"
REQ_AGAIN=$(pyq "d['id']")
ST=$(answer "$T1_T" "$REQ_AGAIN" accept)
check "  and the trainer consents again -> 200" "$ST" "200"
ST=$(code "$B/api/v1/gyms/$GYM/coach-relationships" -H "authorization: Bearer $T1_T")
check "  access is restored" "$(pyq "sum(1 for r in d if r['is_active'])")" "1"
check "  and the old record is still there" "$(pyq "len(d)")" "2"

echo ""
echo "=== 4. the roster ==="
ST=$(code "$B/api/v1/gyms/$GYM/members" -H "authorization: Bearer $HEAD_T")
check "a second owner reads the roster -> 200" "$ST" "200"
check "  everyone in the gym" "$(pyq "len(d)")" "6"
check "  names present" "$(pyq "any(m['display_name']=='Mo Member' for m in d)")" "True"
check "  capacities present" "$(pyq "any('owner' in m['capacities'] for m in d)")" "True"
# A roster endpoint must not become a harvestable contact list.
check "  NO email addresses exposed" "$(pyq "any('email' in m for m in d)")" "False"

# Narrower than can_coach on purpose: who trains at a gym is personal
# information its members did not agree to share with each other.
ST=$(code "$B/api/v1/gyms/$GYM/members" -H "authorization: Bearer $T1_T")
check "trainer reads the roster -> 403" "$ST" "403"
ST=$(code "$B/api/v1/gyms/$GYM/members" -H "authorization: Bearer $M1_T")
check "member reads the roster -> 403" "$ST" "403"
ST=$(code "$B/api/v1/gyms/$GYM/members" -H "authorization: Bearer $OUT_T")
check "outsider reads the roster -> 404" "$ST" "404"

echo ""
echo "=== 5. audit trail ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
for a in coach_relationship.created coach_relationship.ended; do
  check "recorded $a" "$(pyq "any(e['action']=='$a' for e in d)")" "True"
done

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
