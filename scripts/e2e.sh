#!/usr/bin/env bash
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
# Runtime pool uses the unprivileged role so these assertions run WITH row-level
# security enforced, not just with application-level filtering.
export APP_DATABASE_URL="${APP_DATABASE_URL:-postgres://gym_app:gym_app_dev_password@localhost:5455/gym}"
export SERVER_PORT=8099
export SERVER_HOST=127.0.0.1
export JWT_SECRET="dev-only-insecure-secret-change-me-before-any-real-deployment"
# One allowed origin, so the CORS assertions below test the real allowlist
# rather than the empty default. Browser clients are the only thing CORS
# applies to — a native client never sends a preflight, which is exactly why
# gaps here go unnoticed until a web surface exists.
export CORS_ALLOWED_ORIGINS="http://localhost:8210"
export RUST_LOG=warn
B="http://127.0.0.1:$SERVER_PORT"
ORIGIN="http://localhost:8210"

# Always build first — running a stale binary produces meaningless results.
echo "=== building ==="
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

cargo build --bin server 2>&1 | tail -3 || exit 1

./target/debug/server > /tmp/server.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { # check <label> <actual> <expected>
  if [ "$2" = "$3" ]; then echo "  PASS  $1 ($2)"; PASS=$((PASS+1));
  else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi
}
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/body.json -w "%{http_code}" "$@"; }

STAMP=$(date +%s%N)
EMAIL="owner-$STAMP@example.com"
EMAIL2="other-$STAMP@example.com"

echo "=== 1. sign-up (account only) + create gym (step two) ==="
ST=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"correct horse battery staple\",\"display_name\":\"Alex Owner\"}")
check "sign-up returns 201" "$ST" "201"
ACCESS=$(jget "['access_token']" < $VTMP/body.json)
REFRESH1=$(jget "['refresh_token']" < $VTMP/body.json)
check "  no gym implied by sign-up" "$(grep -c '\"gym\"' $VTMP/body.json)" "0"
[ -n "$ACCESS" ] && check "access token issued" "yes" "yes" || check "access token issued" "no" "yes"

# A brand-new account belongs to nothing yet — onboarding asks what to do next.
ST=$(code "$B/api/v1/me" -H "authorization: Bearer $ACCESS")
check "new account has no memberships" "$(jget "['memberships']" < $VTMP/body.json)" "[]"

ST=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $ACCESS" \
  -H 'content-type: application/json' -d '{"name":"Iron Box Strength"}')
check "create gym -> 201" "$ST" "201"
GYM=$(jget "['id']" < $VTMP/body.json)
SLUG=$(jget "['slug']" < $VTMP/body.json)
check "  creator is owner" "$(jget "['capacities']" < $VTMP/body.json)" "['owner']"
echo "  gym=$GYM slug=$SLUG"

check "gym creation requires auth" "$(code -X POST "$B/api/v1/gyms" -H 'content-type: application/json' -d '{"name":"Nope"}')" "401"

echo ""
echo "=== 2. validation & conflicts ==="
ST=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"correct horse battery staple\",\"display_name\":\"Dup\"}")
check "duplicate email -> 409" "$ST" "409"
check "  error code is stable" "$(jget "['code']" < $VTMP/body.json)" "resource.conflict"

ST=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"short-$STAMP@example.com\",\"password\":\"tooshort\",\"display_name\":\"X\"}")
check "short password -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"not-an-email\",\"password\":\"correct horse battery staple\",\"display_name\":\"X\"}")
check "invalid email -> 400" "$ST" "400"

echo ""
echo "=== 3. /me ==="
ST=$(code "$B/api/v1/me" -H "authorization: Bearer $ACCESS")
check "/me with token -> 200" "$ST" "200"
check "  holds the owner capacity" "$(jget "['memberships'][0]['capacities']" < $VTMP/body.json)" "['owner']"
check "  no password leaked" "$(grep -c password_hash $VTMP/body.json)" "0"

check "/me without token -> 401" "$(code "$B/api/v1/me")" "401"
check "/me with garbage token -> 401" "$(code "$B/api/v1/me" -H 'authorization: Bearer nonsense')" "401"
check "/me with wrong scheme -> 401" "$(code "$B/api/v1/me" -H "authorization: Basic $ACCESS")" "401"
check "/me lowercase scheme -> 200" "$(code "$B/api/v1/me" -H "authorization: bearer $ACCESS")" "200"

echo ""
echo "=== 4. exercises (tenant-scoped) ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $ACCESS" \
  -H 'content-type: application/json' -d '{"name":"Back Squat","modality":{"kind":"repetitions"},"notes":"Bar over mid-foot."}')
check "create exercise -> 201" "$ST" "201"
EX=$(jget "['id']" < $VTMP/body.json)

ST=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $ACCESS" \
  -H 'content-type: application/json' -d '{"name":"back squat","modality":{"kind":"duration"}}')
check "duplicate name (case-insensitive) -> 409" "$ST" "409"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $ACCESS" \
  -H 'content-type: application/json' -d '{"name":"   ","modality":{"kind":"distance"}}')
check "blank name -> 400" "$ST" "400"

ST=$(code "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $ACCESS")
check "list exercises -> 200" "$ST" "200"
check "  one exercise present" "$(jget "[0]['name']" < $VTMP/body.json)" "Back Squat"
check "  gym_id not exposed" "$(grep -c gym_id $VTMP/body.json)" "0"

check "list without auth -> 401" "$(code "$B/api/v1/gyms/$GYM/exercises")" "401"

echo ""
echo "=== 5. cross-tenant isolation ==="
ST=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL2\",\"password\":\"another good long passphrase\",\"display_name\":\"Bea\"}")
check "second account created" "$ST" "201"
ACCESS2=$(jget "['access_token']" < $VTMP/body.json)
ST=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $ACCESS2" \
  -H 'content-type: application/json' -d '{"name":"Rival Fitness"}')
check "second gym created" "$ST" "201"
GYM2=$(jget "['id']" < $VTMP/body.json)

check "user B reading gym A -> 404 (not 403)" "$(code "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $ACCESS2")" "404"
check "user A reading gym B -> 404" "$(code "$B/api/v1/gyms/$GYM2/exercises" -H "authorization: Bearer $ACCESS")" "404"
check "user B sees empty own catalogue" "$(code "$B/api/v1/gyms/$GYM2/exercises" -H "authorization: Bearer $ACCESS2")" "200"
check "  and it is empty" "$(jget "" < $VTMP/body.json)" "[]"
check "nonexistent gym -> 404" "$(code "$B/api/v1/gyms/00000000-0000-0000-0000-000000000000/exercises" -H "authorization: Bearer $ACCESS")" "404"
check "malformed gym id -> 404" "$(code "$B/api/v1/gyms/not-a-uuid/exercises" -H "authorization: Bearer $ACCESS")" "404"

echo ""
echo "=== 6. login ==="
ST=$(code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"correct horse battery staple\"}")
check "login -> 200" "$ST" "200"
REFRESH_L=$(jget "['refresh_token']" < $VTMP/body.json)

check "wrong password -> 401" "$(code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\",\"password\":\"wrong password entirely\"}")" "401"
ST=$(code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' -d '{"email":"ghost@example.com","password":"correct horse battery staple"}')
check "unknown email -> 401 (no enumeration)" "$ST" "401"
check "  same code as wrong password" "$(jget "['code']" < $VTMP/body.json)" "auth.unauthenticated"

echo ""
echo "=== 7. refresh rotation + reuse detection ==="
ST=$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json' -d "{\"refresh_token\":\"$REFRESH1\"}")
check "refresh -> 200" "$ST" "200"
REFRESH2=$(jget "['refresh_token']" < $VTMP/body.json)
[ "$REFRESH1" != "$REFRESH2" ] && check "token rotated" "yes" "yes" || check "token rotated" "no" "yes"

check "reusing old refresh -> 401" "$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json' -d "{\"refresh_token\":\"$REFRESH1\"}")" "401"
# Reuse must revoke the whole family, so the *valid* rotated token dies too.
check "family revoked: new token now dead" "$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json' -d "{\"refresh_token\":\"$REFRESH2\"}")" "401"
check "login-issued token also revoked" "$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json' -d "{\"refresh_token\":\"$REFRESH_L\"}")" "401"
check "garbage refresh -> 401" "$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json' -d '{"refresh_token":"deadbeef"}')" "401"

echo ""
echo "=== 8. logout is idempotent ==="
ST=$(code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\",\"password\":\"correct horse battery staple\"}")
R=$(jget "['refresh_token']" < $VTMP/body.json)
check "logout -> 204" "$(code -X POST "$B/api/v1/auth/logout" -H 'content-type: application/json' -d "{\"refresh_token\":\"$R\"}")" "204"
check "logout again -> 204" "$(code -X POST "$B/api/v1/auth/logout" -H 'content-type: application/json' -d "{\"refresh_token\":\"$R\"}")" "204"
check "revoked token cannot refresh" "$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json' -d "{\"refresh_token\":\"$R\"}")" "401"

echo ""
echo "=== 8b. cors preflight (the browser surface) ==="
# Every method the client actually sends must survive a preflight. PUT is the
# one that got missed: profiles and measurements are idempotent upserts, and a
# native client never preflights, so the gap was invisible until the browser
# demo hit it.
preflight() { # preflight <method> <path> -> allowed methods header, or ""
  curl -s -o /dev/null -D - -X OPTIONS "$B$2" \
    -H "origin: $ORIGIN" \
    -H "access-control-request-method: $1" 2>/dev/null \
  | tr -d '\r' | awk -F': ' 'tolower($1)=="access-control-allow-methods"{print $2}'
}
for M in GET POST PUT PATCH DELETE; do
  check "preflight allows $M" \
    "$(preflight "$M" /api/v1/me/profiles/athlete | grep -c "$M")" "1"
done
check "preflight echoes the allowed origin" \
  "$(curl -s -o /dev/null -D - -X OPTIONS "$B/api/v1/me/profiles/athlete" \
      -H "origin: $ORIGIN" -H 'access-control-request-method: PUT' \
    | tr -d '\r' | awk -F': ' 'tolower($1)=="access-control-allow-origin"{print $2}')" \
  "$ORIGIN"
# An origin outside the allowlist gets no permission at all — never a wildcard.
check "an unlisted origin is refused" \
  "$(curl -s -o /dev/null -D - -X OPTIONS "$B/api/v1/me/profiles/athlete" \
      -H 'origin: https://evil.example' -H 'access-control-request-method: PUT' \
    | tr -d '\r' | grep -ci 'access-control-allow-origin')" \
  "0"

echo ""
echo "=== 9. openapi ==="
curl -fsS "$B/api-docs/openapi.json" > $VTMP/oapi.json
# A deliberate change-detector: the count is pinned so that adding or removing a
# route is a conscious edit here rather than a silent change to the public API.
# 12 → 19 when programme authoring landed (7 endpoints).
# 19 → 21 when coach relationships landed (2 paths, 3 operations).
# 21 → 22 when the gym roster landed.
# 22 → 24 when programme assignments landed (list/assign + withdraw).
# 24 → 28 when workout execution landed (sessions, detail, sets, finish).
# 28 → 29 when per-exercise history landed.
# 29 → 33 when profiles landed (me/profiles ×3, member athlete-profile).
# 33 → 36 when body measurements landed.
# 36 → 38 when goals landed.
# 38 → 39 when recommendations landed.
# 39 → 45 when billing landed (plans ×2, subscriptions, invoices, void, payments),
# → 46 with the entitlement read model.
# 46 → 51 when self-service card payment and door check-ins landed
#   (invoice checkout-session, the processor webhook, entry pass, scan, recent).
# 51 → 72 across the ADR-0024..0029 work:
#   +2  exercise curation (pending queue, curate)
#   +4  coaching requests + the trainer directory (ADR-0025)
#   +4  open registration (open list, join, settings read/write — ADR-0026)
#   +1  cancel a subscription (found missing once billing actually recurred)
#   +4  password reset, verification, send-verification, verify-email (ADR-0029)
#   +6  the operating calendar (ADR-0015: resolve, hours r/w, override set,
#       override clear, availability r/w, bookable)
#   -2  the two invitation paths, removed with the feature (ADR-0031)
#   +1  PUT a member's capacities — what replaced them
#   +2  POST a staff account, and change-password (ADR-0032)
# The two `/pay/...` card-page routes are NOT counted: they serve HTML to a
# browser and are deliberately outside the OpenAPI document (ADR-0028).
check "openapi paths" "$("$PY" -c "import json;print(len(json.load(open('$VTMP/oapi.json', encoding='utf-8'))['paths']))")" "73"
# Billing must be published as its own tag, not folded into gyms.
# 8: the invoice checkout-session path, plus cancelling a subscription.
check "billing routes in openapi" "$("$PY" -c "import json;p=json.load(open('$VTMP/oapi.json', encoding='utf-8'))['paths'];print(sum(1 for k in p if any(w in k for w in ('plans','subscriptions','invoices'))))")" "8"
# The programme routes must actually be published, not merely counted.
# 6 authoring paths + 2 assignment paths.
check "programme routes in openapi" "$("$PY" -c "import json;p=json.load(open('$VTMP/oapi.json', encoding='utf-8'))['paths'];print(sum(1 for k in p if 'program' in k))")" "8"
check "bearer scheme registered" "$("$PY" -c "import json;print('bearer' in json.load(open('$VTMP/oapi.json', encoding='utf-8')).get('components',{}).get('securitySchemes',{}))")" "True"

echo ""
echo "=== 10. db state ==="
docker exec gym-postgres psql -U gym -d gym -tAc \
  "SELECT 'sessions revoked: '||count(*) FROM sessions WHERE revoked_at IS NOT NULL;"
docker exec gym-postgres psql -U gym -d gym -tAc \
  "SELECT 'token_hash is bytea, never raw: '||(count(*)=0)::text FROM sessions WHERE encode(token_hash,'hex') ~ '^[0-9a-f]{64}$' AND length(token_hash)<>32;"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ] || { echo "--- server log ---"; tail -20 /tmp/server.log; }
exit "$FAIL"
