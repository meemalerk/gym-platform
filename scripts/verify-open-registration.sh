#!/usr/bin/env bash
# Open registration — the owner-held door (ADR-0026).
#
# One rule matters more than all the others here, so it is tested from several
# angles: **the open door admits members and only members.** A switch that let
# a stranger sign up as an owner would be the worst bug this codebase could
# have, and it is exactly the kind that a request-body parameter introduces
# quietly. The capacity is hard-coded server-side; these assertions are what
# stop someone "helpfully" making it configurable later.
#
# Second rule: a closed gym must not be discoverable. "No such gym" and "that
# gym is closed" return the same 404, or the endpoint becomes a directory of
# every gym on the deployment.
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
export SERVER_PORT=8104
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
./target/debug/server > /tmp/server-openreg.log 2>&1 &
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
caps_of() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null
            pyq "sorted(m['capacities'] for m in d['memberships'] if m['gym_id'] == '$2')"; }
open_door() { code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "{\"open_registration\":$2}"; }

echo "=== setup: a closed gym ==="
OWNER_T=$(signup "or-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Open Door Gym $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)

echo ""
echo "=== 1. closed by default ==="
ST=$(code "$B/api/v1/gyms/$GYM/settings" -H "authorization: Bearer $OWNER_T")
check "an owner may read the settings" "$ST" "200"
check "  a new gym is closed" "$(pyq "d['open_registration']")" "False"

STRANGER_T=$(signup "or-stranger-$S@example.com" "Sam Stranger")
ST=$(code "$B/api/v1/gyms/open" -H "authorization: Bearer $STRANGER_T")
check "the open list is readable by an account with no gym" "$ST" "200"
check "  and the closed gym is not in it" "$(pyq "any(g['id'] == '$GYM' for g in d)")" "False"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $STRANGER_T")
check "joining a closed gym is refused" "$ST" "404"
check "  and refused as NOT FOUND, so closure is not discoverable" "$ST" "404"

echo ""
echo "=== 2. who may open the door ==="
MEMBER_T=$(signup "or-member-$S@example.com" "Mo Member")
# ADR-0031: no invitations. The door is the only way in, and standing is set
# afterwards — so the setup for "who may open the door" has to open it, let
# them in, promote, and close it again before the checks below run.
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $MEMBER_T" >/dev/null

TR_T=$(signup "or-tr-$S@example.com" "Tara Trainer")
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $TR_T" >/dev/null
code -X PUT "$B/api/v1/gyms/$GYM/members/$(uid "$TR_T")/capacities" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"capacities":["trainer"]}' >/dev/null
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":false}' >/dev/null

ST=$(open_door "$MEMBER_T" true)
check "a member may not open the door" "$ST" "403"

# Deliberately narrower than can_coach: who may walk into the gym is a settings
# question, not a coaching one. Since ADR-0036 the trainer is the only staff
# rung below owner, so this is the whole of "staff, but not a manager".
ST=$(open_door "$TR_T" true)
check "a trainer may not either" "$ST" "403"
ST=$(code "$B/api/v1/gyms/$GYM/settings" -H "authorization: Bearer $TR_T")
check "  nor read the setting" "$ST" "403"

ST=$(open_door "$OWNER_T" true)
check "an owner may open it" "$ST" "200"
check "  and it reads back open" "$(pyq "d['open_registration']")" "True"

echo ""
echo "=== 3. walking through ==="
ST=$(code "$B/api/v1/gyms/open" -H "authorization: Bearer $STRANGER_T")
check "the gym now appears in the open list" "$(pyq "any(g['id'] == '$GYM' for g in d)")" "True"
check "  by name, so a person can recognise it" \
  "$(pyq "next(g['name'] for g in d if g['id'] == '$GYM')")" "Open Door Gym $S"
check "  and the listing says nothing about who trains there" \
  "$(pyq "sorted(next(g for g in d if g['id'] == '$GYM').keys())")" "['id', 'name', 'slug']"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $STRANGER_T")
check "a stranger may join" "$ST" "204"

# THE assertion. If this ever reads anything but ['member'], the open door has
# become a privilege escalation.
check "  as a plain member, and nothing more" "$(caps_of "$STRANGER_T" "$GYM")" "[['member']]"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $STRANGER_T")
check "joining twice is refused" "$ST" "409"

# The open door must not be a way to ask for more than it gives. The route
# takes no capacity parameter at all; this proves a body cannot smuggle one in.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $STRANGER_T" \
  -H 'content-type: application/json' -d '{"capacities":["owner"]}')
check "a forged capacity in the body changes nothing" "$ST" "409"
check "  they are still just a member" "$(caps_of "$STRANGER_T" "$GYM")" "[['member']]"

SNEAK_T=$(signup "or-sneak-$S@example.com" "Sneaky Pete")
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $SNEAK_T" \
  -H 'content-type: application/json' -d '{"capacity":"owner","capacities":["owner","trainer"]}' >/dev/null
check "and a fresh account cannot escalate on the way in" "$(caps_of "$SNEAK_T" "$GYM")" "[['member']]"

echo ""
echo "=== 4. closing it again ==="
ST=$(open_door "$OWNER_T" false)
check "an owner may close the door" "$(pyq "d['open_registration']")" "False"

LATE_T=$(signup "or-late-$S@example.com" "Late Larry")
ST=$(code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $LATE_T")
check "someone arriving after it closed is refused" "$ST" "404"

# Closing does not evict. People who joined while it was open stay.
check "  but those already in stay in" "$(caps_of "$STRANGER_T" "$GYM")" "[['member']]"

echo ""
echo "=== 5. the door is the only way in ==="
# ADR-0026 shipped "both doors"; ADR-0031 closed one of them. There is no
# invitation path left, so a closed gym admits nobody at all — not even
# somebody the owner wants as staff. Staff are made from members, and a member
# has to get in first.
STAFF_T=$(signup "or-staff-$S@example.com" "Tariq Trainer")
ST=$(code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $STAFF_T")
check "a closed gym admits nobody, however senior they are to be" "$ST" "404"

ST=$(code -X POST "$B/api/v1/invitations/accept" -H "authorization: Bearer $STAFF_T" \
  -H 'content-type: application/json' -d '{"token":"anything"}')
check "  and the invitation endpoint is gone entirely" "$ST" "404"

# Open it, let them in, promote: the whole of staff onboarding, in three calls.
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $STAFF_T" >/dev/null
ST=$(code -X PUT "$B/api/v1/gyms/$GYM/members/$(uid "$STAFF_T")/capacities" \
  -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
  -d '{"capacities":["trainer"]}')
check "a member can be promoted to staff" "$ST" "200"
check "  and holds exactly what was set" "$(caps_of "$STAFF_T" "$GYM")" "[['trainer']]"
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":false}' >/dev/null

echo ""
echo "=== 6. the switch is audited ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
check "opening was recorded" "$(pyq "any(e['action'] == 'gym.registration_opened' for e in d)")" "True"
check "closing was recorded" "$(pyq "any(e['action'] == 'gym.registration_closed' for e in d)")" "True"
check "and so was each self-join" \
  "$(pyq "sum(1 for e in d if e['action'] == 'capacity.granted' and (e.get('metadata') or {}).get('reason') == 'open_registration')")" "5"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
