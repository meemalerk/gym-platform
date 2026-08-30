#!/usr/bin/env bash
# Proves the audit trail records every tenant mutation, is readable only by gym
# managers, and cannot be rewritten by the application role.
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
export SERVER_PORT=8097
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
./target/debug/server > /tmp/server-audit.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then echo "  PASS  $1 ($2)"; PASS=$((PASS+1));
          else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi; }
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
pyq() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.load(open('$VTMP/b.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/b.json -w "%{http_code}" "$@"; }
# Container name locally, a real client on CI - see the reasoning there.
. scripts/lib/psql.sh
PSQL=("${PSQL_OWNER[@]}")

S=$(date +%s%N); PW="correct horse battery staple"
OWNER="aud-owner-$S@example.com"; COACH="aud-coach-$S@example.com"

echo "=== perform a series of auditable actions ==="
OWNER_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$OWNER\",\"password\":\"$PW\",\"display_name\":\"Olive Owner\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Audited Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)
code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Deadlift","modality":{"kind":"repetitions"}}' >/dev/null
COACH_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$COACH\",\"password\":\"$PW\",\"display_name\":\"Casey Coach\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $COACH_T" >/dev/null
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
COACH_ID=$(code "$B/api/v1/me" -H "authorization: Bearer $COACH_T" >/dev/null; jget "['user']['id']" < $VTMP/b.json)
code -X PUT "$B/api/v1/gyms/$GYM/members/$COACH_ID/capacities" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"capacities":["trainer"]}' >/dev/null
echo "  gym=$GYM"

echo ""
echo "=== 1. every mutation left a trace ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
check "audit readable by owner -> 200" "$ST" "200"
ACTIONS=$(pyq "sorted({e['action'] for e in d})")
echo "  actions: $ACTIONS"
for a in gym.created exercise.created gym.registration_opened capacity.granted; do
  check "  recorded $a" "$(pyq "any(e['action']=='$a' for e in d)")" "True"
done
check "  actor is named" "$(pyq "any(e['actor_name']=='Olive Owner' for e in d)")" "True"
check "  metadata captured" "$(pyq "any(e.get('metadata',{}).get('name')=='Deadlift' for e in d)")" "True"

echo ""
echo "=== 2. the log never contains secrets ==="
check "no bearer token in the trail" "$(grep -ci "$OWNER_T" $VTMP/b.json)" "0"
check "no password_hash" "$(grep -c password_hash $VTMP/b.json)" "0"

echo ""
echo "=== 3. reading it is restricted ==="
check "trainer -> 403" "$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $COACH_T")" "403"
check "unauthenticated -> 401" "$(code "$B/api/v1/gyms/$GYM/audit")" "401"

echo ""
echo "=== 4. the trail is tenant-isolated ==="
OTHER_T=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"aud-other-$S@example.com\",\"password\":\"$PW\",\"display_name\":\"Other\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json)
check "outsider -> 404" "$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OTHER_T")" "404"

echo ""
echo "=== 5. append-only: the app role cannot rewrite history ==="
ROWS=$("${PSQL[@]}" -c "SELECT count(*) FROM audit_log WHERE gym_id='$GYM';")
check "entries exist" "$([ "$ROWS" -ge 5 ] && echo yes || echo no)" "yes"

UPD=$("${PSQL_APP[@]}" <<SQL 2>&1
BEGIN;
SELECT set_config('app.current_gym','$GYM',true);
UPDATE audit_log SET action='exercise.created' WHERE gym_id='$GYM';
COMMIT;
SQL
)
if echo "$UPD" | grep -qi "permission denied"; then
  check "UPDATE denied to app role" "denied" "denied"
else
  check "UPDATE denied to app role" "ALLOWED: $UPD" "denied"
fi

DEL=$("${PSQL_APP[@]}" <<SQL 2>&1
BEGIN;
SELECT set_config('app.current_gym','$GYM',true);
DELETE FROM audit_log WHERE gym_id='$GYM';
COMMIT;
SQL
)
if echo "$DEL" | grep -qi "permission denied"; then
  check "DELETE denied to app role" "denied" "denied"
else
  check "DELETE denied to app role" "ALLOWED: $DEL" "denied"
fi

AFTER=$("${PSQL[@]}" -c "SELECT count(*) FROM audit_log WHERE gym_id='$GYM';")
check "history intact after both attempts" "$AFTER" "$ROWS"

echo ""
echo "=== 6. atomicity: a rejected mutation leaves no entry ==="
BEFORE=$("${PSQL[@]}" -c "SELECT count(*) FROM audit_log WHERE gym_id='$GYM' AND action='exercise.created';")
code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Deadlift","modality":{"kind":"duration"}}' >/dev/null
NOW=$("${PSQL[@]}" -c "SELECT count(*) FROM audit_log WHERE gym_id='$GYM' AND action='exercise.created';")
check "duplicate exercise rolled back, no audit row" "$NOW" "$BEFORE"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ] || tail -15 /tmp/server-audit.log
exit "$FAIL"
