#!/usr/bin/env bash
# Standing — how somebody becomes staff, now that invitations are gone
# (ADR-0031).
#
# This suite replaces verify-invitations.sh, and it is testing the same
# property that one was: **you cannot give yourself, or be given, more standing
# than the person granting it has.** The mechanism changed; the thing that must
# not break did not.
#
# Four rules, each of which is a way a gym could be taken over or locked out:
#
#   1. Only owners and admins may change anybody's standing.
#   2. Only an OWNER may grant or remove `owner`. An admin who could promote
#      themselves is an owner with extra steps.
#   3. The last owner cannot step down — a gym with no owner cannot appoint one.
#   4. Setting standing REPLACES it, so demotion is the same call as promotion
#      and there is no window where somebody holds both or neither.
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
export SERVER_PORT=8112
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
./target/debug/server > /tmp/server-capacities.log 2>&1 &
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

S=$(date +%s%N); PW="correct horse battery staple"

signup() { code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$1\",\"password\":\"$PW\",\"display_name\":\"$2\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json; }
uid() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/b.json; }
caps_of() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null
            pyq "sorted(m['capacities'] for m in d['memberships'] if m['gym_id'] == '$2')"; }
setcaps() { code -X PUT "$B/api/v1/gyms/$GYM/members/$2/capacities" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' -d "{\"capacities\":$3}"; }

echo "=== setup: a gym with an open door ==="
OWNER_T=$(signup "cap-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Standing Gym $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)
OWNER=$(uid "$OWNER_T")
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null

MEMBER_T=$(signup "cap-member-$S@example.com" "Mo Member")
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $MEMBER_T" >/dev/null
MEMBER=$(uid "$MEMBER_T")
echo "  gym=$GYM"

echo ""
echo "=== 1. the door still only ever grants member ==="
check "joining grants exactly member" "$(caps_of "$MEMBER_T" "$GYM")" "[['member']]"

echo ""
echo "=== 2. an owner promotes a member to staff ==="
ST=$(setcaps "$OWNER_T" "$MEMBER" '["member","trainer"]')
check "owner may set standing -> 200" "$ST" "200"
# Read the PUT's own body BEFORE anything else touches b.json.
check "  the response says what they now hold" \
  "$(pyq "sorted(d['capacities'])")" "['member', 'trainer']"
# Capacities come back in the capability LADDER's order (owner, admin, head
# coach, trainer, member), not alphabetically — `Capabilities::new` sorts by
# the enum. Asserted rather than sorted away, because a client rendering
# badges leans on the senior one coming first.
check "  and both capacities stuck" "$(caps_of "$MEMBER_T" "$GYM")" "[['trainer', 'member']]"

echo ""
echo "=== 3. setting REPLACES rather than adds ==="
ST=$(setcaps "$OWNER_T" "$MEMBER" '["member"]')
check "demotion is the same call -> 200" "$ST" "200"
check "  trainer is gone" "$(caps_of "$MEMBER_T" "$GYM")" "[['member']]"
# Re-granting must not leave two live rows saying the same thing.
setcaps "$OWNER_T" "$MEMBER" '["member","trainer"]' >/dev/null
check "  re-granting does not duplicate" "$(caps_of "$MEMBER_T" "$GYM")" "[['trainer', 'member']]"

echo ""
echo "=== 4. who may change standing at all ==="
# ADR-0036 left one rung above trainer, so this is now about who is refused
# rather than about where the line between two kinds of manager fell.
TR_T=$(signup "cap-tr-$S@example.com" "Tara Trainer")
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $TR_T" >/dev/null
TR=$(uid "$TR_T")
setcaps "$OWNER_T" "$TR" '["trainer"]' >/dev/null

ST=$(setcaps "$TR_T" "$MEMBER" '["member","trainer"]')
check "a trainer may not — they coach clients, they do not run the gym" "$ST" "403"
ST=$(setcaps "$MEMBER_T" "$MEMBER" '["owner"]')
check "a member may not promote themselves" "$ST" "403"
check "  and nothing changed" "$(caps_of "$MEMBER_T" "$GYM")" "[['trainer', 'member']]"

echo ""
echo "=== 5. the removed rungs are gone for good (ADR-0036) ==="
# `admin` and `head_coach` are refused at three layers: the API rejects the
# string, `Capacity::parse` returns None for it (a domain unit test), and a
# CHECK constraint stops one reaching the table (asserted by the migration).
# This is the outermost layer, which is the one a client can actually reach.
for gone in admin head_coach; do
  ST=$(setcaps "$OWNER_T" "$MEMBER" "[\"$gone\"]")
  check "even an owner cannot grant '$gone' now"     "$([ "$ST" = "400" ] || [ "$ST" = "422" ] && echo refused)" "refused"
done
check "  and the member is unchanged" "$(caps_of "$MEMBER_T" "$GYM")" "[['trainer', 'member']]"

echo ""
echo "=== 6. a gym must keep an owner ==="
ST=$(setcaps "$OWNER_T" "$OWNER" '["member"]')
check "the last owner may not step down" "$ST" "400"
check "  and is still the owner" "$(caps_of "$OWNER_T" "$GYM")" "[['owner']]"

# With a second owner in place, stepping down is allowed.
OWNER2_T=$(signup "cap-owner2-$S@example.com" "Otto Owner")
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $OWNER2_T" >/dev/null
OWNER2=$(uid "$OWNER2_T")
ST=$(setcaps "$OWNER_T" "$OWNER2" '["owner"]')
check "an owner may appoint another owner" "$ST" "200"
ST=$(setcaps "$OWNER_T" "$OWNER" '["member"]')
check "  and may then step down" "$ST" "200"
check "  landing as a plain member" "$(caps_of "$OWNER_T" "$GYM")" "[['member']]"

echo ""
echo "=== 7. refusals that should not be informative ==="
STRANGER_T=$(signup "cap-stranger-$S@example.com" "Sam Stranger")
STRANGER=$(uid "$STRANGER_T")
ST=$(setcaps "$OWNER2_T" "$STRANGER" '["trainer"]')
check "somebody who never joined cannot be promoted -> 404" "$ST" "404"
ST=$(setcaps "$OWNER2_T" "$MEMBER" '[]')
check "standing cannot be emptied -> 400" "$ST" "400"
ST=$(setcaps "$OWNER2_T" "$MEMBER" '["superuser"]')
check "an unknown capacity is a bad request, not a 500" "$ST" "400"

echo ""
echo "=== 8. creating a staff account outright (ADR-0032) ==="
# The fast path: instead of waiting for somebody to sign up and walk through
# the door so they can be promoted, the owner makes the account and the
# standing in one request and hands over a generated password.
mkstaff() { code -X POST "$B/api/v1/gyms/$GYM/staff" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' \
  -d "{\"email\":\"$2\",\"display_name\":\"$3\",\"capacities\":$4}"; }

ST=$(mkstaff "$OWNER2_T" "cap-newcoach-$S@example.com" "Nia Coach" '["trainer","member"]')
check "an owner may create a staff account" "$ST" "201"
NEW_PW=$(pyq "d['temporary_password']")
NEW_ID=$(pyq "d['user_id']")
check "  it comes back with a one-time password" "$(pyq "len(d['temporary_password']) >= 12")" "True"
check "  holding what was asked for" "$(pyq "sorted(d['capacities'])")" "['member', 'trainer']"

# The whole point: that password works, and the standing is real.
ST=$(code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' \
  -d "{\"email\":\"cap-newcoach-$S@example.com\",\"password\":\"$NEW_PW\"}")
check "the new account can sign in with it" "$ST" "200"
NEW_T=$(pyq "d['access_token']")
NEW_R=$(pyq "d['refresh_token']")
check "  and holds its standing in the gym" "$(caps_of "$NEW_T" "$GYM")" "[['trainer', 'member']]"

# They appear on the roster like anybody else — no second kind of person.
code "$B/api/v1/gyms/$GYM/members" -H "authorization: Bearer $OWNER2_T" >/dev/null
check "  and is on the roster" "$(pyq "any(m['user_id'] == '$NEW_ID' for m in d)")" "True"

ST=$(mkstaff "$OWNER2_T" "cap-newcoach-$S@example.com" "Nia Again" '["trainer"]')
check "the same address twice is a conflict, not a second account" "$ST" "409"

ST=$(mkstaff "$MEMBER_T" "cap-nope-$S@example.com" "Nope" '["trainer"]')
check "a member may not create staff" "$ST" "403"
ST=$(mkstaff "$TR_T" "cap-nope2-$S@example.com" "Nope" '["trainer"]')
check "  nor may a trainer" "$ST" "403"

# The owner rule held on creation too, so that "make a staff account" could not
# become a way for a non-owner manager to mint an owner and sign in as it.
# ADR-0036 removed the only non-owner manager, so what is left to assert is the
# positive: an owner may create staff, including another owner.
ST=$(mkstaff "$OWNER2_T" "cap-ownermade-$S@example.com" "Made By Owner" '["trainer"]')
check "an owner may create ordinary staff" "$ST" "201"
ST=$(mkstaff "$OWNER2_T" "cap-ownermade2-$S@example.com" "Second Owner" '["owner"]')
check "  and may create another owner" "$ST" "201"

ST=$(mkstaff "$OWNER2_T" "cap-empty-$S@example.com" "No Standing" '[]')
check "a created account must hold something" "$ST" "400"
ST=$(mkstaff "$OWNER2_T" "not-an-address" "Bad Address" '["trainer"]')
check "a malformed address is refused" "$ST" "400"
ST=$(mkstaff "$OWNER2_T" "cap-bad-$S@example.com" "Bad Capacity" '["superuser"]')
check "an unknown capacity is a bad request, not a 500" "$ST" "400"

echo ""
echo "=== 9. getting off the password somebody else chose ==="
# Staff accounts start on a generated password their manager read out, so
# there has to be a way off it that does not go through an email nobody sends.
chpw() { code -X POST "$B/api/v1/auth/change-password" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' \
  -d "{\"current_password\":\"$2\",\"new_password\":\"$3\"}"; }

ST=$(chpw "$NEW_T" "the wrong one entirely" "a brand new password here")
check "the current password is required" "$ST" "401"
ST=$(chpw "$NEW_T" "$NEW_PW" "short")
check "the new one still has to be long enough" "$ST" "400"

ST=$(chpw "$NEW_T" "$NEW_PW" "a brand new password here")
check "changing it works" "$ST" "204"

ST=$(code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' \
  -d "{\"email\":\"cap-newcoach-$S@example.com\",\"password\":\"a brand new password here\"}")
check "  the new password signs in" "$ST" "200"
ST=$(code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' \
  -d "{\"email\":\"cap-newcoach-$S@example.com\",\"password\":\"$NEW_PW\"}")
check "  the one their manager knows does not" "$ST" "401"

# Every session, everywhere — the same contract a reset has, because it is the
# same event from the account's point of view.
#
# The REFRESH token is what dies. The access token is a short-lived stateless
# JWT and is not checked against the session table, so it keeps working until
# it expires — that is the trade the design already made (ADR-0029), and
# asserting otherwise here would be asserting something this system does not
# do. What matters is that the old session cannot be renewed.
ST=$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json'   -d "{\"refresh_token\":\"$NEW_R\"}")
check "  and the old session cannot be renewed" "$ST" "401"

echo ""
echo "=== 10. the invitation endpoints are gone ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invitations" -H "authorization: Bearer $OWNER2_T" \
  -H 'content-type: application/json' -d '{"email":"x@example.com","capacities":["trainer"]}')
check "POST /invitations -> 404" "$ST" "404"
ST=$(code "$B/api/v1/gyms/$GYM/invitations" -H "authorization: Bearer $OWNER2_T")
check "GET  /invitations -> 404" "$ST" "404"
ST=$(code -X POST "$B/api/v1/invitations/accept" -H "authorization: Bearer $MEMBER_T" \
  -H 'content-type: application/json' -d '{"token":"anything"}')
check "POST /invitations/accept -> 404" "$ST" "404"

echo ""
echo "=== 11. every change is on the record ==="
code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER2_T" >/dev/null
check "standing changes are audited" \
  "$(pyq "sum(1 for e in d if e['action'] == 'capacity.granted' and (e.get('metadata') or {}).get('reason') == 'standing_changed') >= 6")" "True"
check "creating a staff account is audited" \
  "$(pyq "any(e['action'] == 'staff.created' for e in d)")" "True"
check "  and self-joins are told apart from grants" \
  "$(pyq "any(e['action'] == 'capacity.granted' and (e.get('metadata') or {}).get('reason') == 'open_registration' for e in d)")" "True"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
