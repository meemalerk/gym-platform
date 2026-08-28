#!/usr/bin/env bash
# Password reset, email verification and login throttling (ADR-0029).
#
# Three holes this closes, and the suite is organised around what each one
# must NOT do as much as what it must:
#
#   §1-3  a forgotten password used to lock an account out permanently. The
#         reset must work — and must not become a way to ask whether an
#         address is registered, must not survive being used twice, and must
#         not leave the attacker's existing session alive.
#   §4    an unverified address was indistinguishable from a verified one.
#         Verification must not become a barrier to signing in.
#   §5    the login endpoint counted nothing. It must now throttle — and must
#         not lock out a whole office because one person forgot a password.
#
# The reset link is read back out of `sent_emails`, which is exactly why the
# recording sender exists: without it this flow could not be tested at all
# without an SMTP account.
cd "$(dirname "$0")/.." || exit 1

export VTMP="target/verify-tmp"
mkdir -p "$VTMP"

if [ -z "${PY:-}" ]; then
  for candidate in python3 python py; do
    if command -v "$candidate" >/dev/null 2>&1  && "$candidate" -c 'import sys; sys.exit(0 if sys.version_info[0] == 3 else 1)' >/dev/null 2>&1; then
      PY="$candidate"; break
    fi
  done
fi
if [ -z "${PY:-}" ]; then echo "no python3 on PATH — cannot parse API responses" >&2; exit 1; fi
export PY

export DATABASE_URL="postgres://gym:gym_dev_password@localhost:5455/gym"
export APP_DATABASE_URL="${APP_DATABASE_URL:-postgres://gym_app:gym_app_dev_password@localhost:5455/gym}"
export SERVER_PORT=8110
export SERVER_HOST=127.0.0.1
export JWT_SECRET="dev-only-insecure-secret-change-me-before-any-real-deployment"
export RUST_LOG=warn
B="http://127.0.0.1:$SERVER_PORT"

reap_stale_servers() {
  if command -v taskkill >/dev/null 2>&1; then
    taskkill //F //IM server.exe >/dev/null 2>&1 || true
  elif command -v pkill >/dev/null 2>&1; then
    pkill -9 -f "$PWD/target/debug/server" 2>/dev/null || true
  fi
}
reap_stale_servers

cargo build --bin server 2>&1 | tail -2 || exit 1
./target/debug/server > /tmp/server-auth.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then PASS=$((PASS+1));
          else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi; }
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
pyq() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.load(open('$VTMP/b.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/b.json -w "%{http_code}" "$@"; }
psql_q() { docker exec gym-postgres psql -U gym -d gym -tAc "$1" 2>/dev/null | tr -d '\r'; }

S=$(date +%s%N); PW="correct horse battery staple"; NEWPW="an entirely different passphrase"
EMAIL="ah-user-$S@example.com"

login() { code -X POST "$B/api/v1/auth/login" -H 'content-type: application/json'  -d "{\"email\":\"$1\",\"password\":\"$2\"}"; }
# The link the platform would have emailed. Reading it here is the whole point
# of recording sent mail.
link_for() { psql_q "SELECT substring(body from 'token=([A-Za-z0-9]+)')
                     FROM sent_emails WHERE to_email = '$1' AND kind = '$2'
                     ORDER BY sent_at DESC LIMIT 1"; }

# Clear this HOST's recent failures before testing the per-address counter.
#
# Not papering over anything — the opposite. The per-IP counter is genuinely
# global to the host, and a full `all-check.sh` run makes well over fifty
# failed logins from 127.0.0.1 across the other suites, so by the time this one
# starts the IP is legitimately locked. That is the throttle working. It also
# makes this suite's result depend on what ran before it, which is not a
# property a test should have — so it establishes its own precondition and
# says why.
#
# Worth noting as a real operational consequence: a large gym behind one office
# NAT shares an address, and fifty failures in fifteen minutes is not
# unimaginable on a Monday. The per-address counter is the one that protects an
# account; this one is a blunt secondary signal, and its limit is set high for
# exactly that reason.
psql_q "DELETE FROM login_attempts WHERE attempted_at >= now() - interval '20 minutes'" >/dev/null

echo "=== setup: an account ==="
ST=$(code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json'  -d "{\"email\":\"$EMAIL\",\"password\":\"$PW\",\"display_name\":\"Ada Hardening\"}")
check "sign-up works" "$ST" "201"
TOKEN=$(pyq "d['access_token']")
REFRESH=$(pyq "d['refresh_token']")

echo ""
echo "=== 1. asking for a reset says nothing about who exists ==="
ST=$(code -X POST "$B/api/v1/auth/forgot-password" -H 'content-type: application/json'  -d "{\"email\":\"$EMAIL\"}")
check "a registered address is accepted" "$ST" "202"

# The load-bearing one. A different status here would turn this endpoint into
# a membership oracle: post an address, read the code, learn who trains here.
ST=$(code -X POST "$B/api/v1/auth/forgot-password" -H 'content-type: application/json'  -d '{"email":"nobody-at-all-9f3a@example.com"}')
check "an unregistered one is accepted identically" "$ST" "202"
ST=$(code -X POST "$B/api/v1/auth/forgot-password" -H 'content-type: application/json'  -d '{"email":"not even an address"}')
check "and so is a malformed one" "$ST" "202"

check "only the real address got mail"  "$(psql_q "SELECT count(*) FROM sent_emails WHERE to_email LIKE 'nobody-at-all%'")" "0"

RESET=$(link_for "$EMAIL" "password_reset")
check "a reset link was recorded for the real one"  "$([ -n "$RESET" ] && echo yes)" "yes"

# The raw token must never be stored — only its hash, exactly like refresh
# tokens and invitations.
check "the token is stored only as a hash"  "$(psql_q "SELECT count(*) FROM auth_tokens WHERE encode(token_hash,'hex') = '$RESET'")" "0"
check "  and a row exists for it" "$(psql_q "SELECT count(*) FROM auth_tokens t JOIN users u ON u.id = t.user_id WHERE u.email = '$EMAIL' AND t.purpose='password_reset' AND t.used_at IS NULL")" "1"

echo ""
echo "=== 2. using the link ==="
ST=$(code -X POST "$B/api/v1/auth/reset-password" -H 'content-type: application/json'  -d "{\"token\":\"$RESET\",\"password\":\"short\"}")
check "a too-short password is refused" "$ST" "400"
check "  and the link still works afterwards" "$(psql_q "SELECT count(*) FROM auth_tokens t JOIN users u ON u.id = t.user_id WHERE u.email = '$EMAIL' AND t.purpose='password_reset' AND t.used_at IS NULL")" "1"

ST=$(code -X POST "$B/api/v1/auth/reset-password" -H 'content-type: application/json'  -d "{\"token\":\"$RESET\",\"password\":\"$NEWPW\"}")
check "the reset succeeds" "$ST" "204"

ST=$(login "$EMAIL" "$NEWPW")
check "  the new password works" "$ST" "200"
ST=$(login "$EMAIL" "$PW")
check "  the old one does not" "$ST" "401"

echo ""
echo "=== 3. a reset link is single-use, and ends every session ==="
ST=$(code -X POST "$B/api/v1/auth/reset-password" -H 'content-type: application/json'  -d "{\"token\":\"$RESET\",\"password\":\"yet another passphrase\"}")
check "the same link cannot be used twice" "$ST" "400"

# The point of revoking: someone resetting a password believes it is
# compromised. Leaving the attacker's refresh token alive makes it theatre.
ST=$(code -X POST "$B/api/v1/auth/refresh" -H 'content-type: application/json'  -d "{\"refresh_token\":\"$REFRESH\"}")
check "the session held before the reset is dead" "$ST" "401"

# A second outstanding link — one an attacker requested — must die too.
code -X POST "$B/api/v1/auth/forgot-password" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\"}" >/dev/null
FIRST=$(link_for "$EMAIL" "password_reset")
code -X POST "$B/api/v1/auth/forgot-password" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\"}" >/dev/null
SECOND=$(link_for "$EMAIL" "password_reset")
check "two outstanding links can exist" "$([ "$FIRST" != "$SECOND" ] && echo yes)" "yes"

code -X POST "$B/api/v1/auth/reset-password" -H 'content-type: application/json'  -d "{\"token\":\"$SECOND\",\"password\":\"$NEWPW\"}" >/dev/null
ST=$(code -X POST "$B/api/v1/auth/reset-password" -H 'content-type: application/json'  -d "{\"token\":\"$FIRST\",\"password\":\"a third passphrase entirely\"}")
check "  using one kills the other" "$ST" "400"

ST=$(code -X POST "$B/api/v1/auth/reset-password" -H 'content-type: application/json'  -d '{"token":"completely-made-up","password":"a long enough passphrase"}')
check "a forged link is refused the same way" "$ST" "400"

echo ""
echo "=== 4. email verification does not gate anything ==="
FRESH=$(login "$EMAIL" "$NEWPW" >/dev/null; pyq "d['access_token']")
check "an unverified account signs in perfectly well"  "$(psql_q "SELECT email_verified_at IS NULL FROM users WHERE email = '$EMAIL'")" "t"

ST=$(code -X POST "$B/api/v1/auth/send-verification" -H "authorization: Bearer $FRESH")
check "a verification link can be requested" "$ST" "202"
VERIFY=$(link_for "$EMAIL" "email_verification")
check "  and was recorded" "$([ -n "$VERIFY" ] && echo yes)" "yes"

ST=$(code -X POST "$B/api/v1/auth/verify-email" -H 'content-type: application/json'  -d "{\"token\":\"$VERIFY\"}")
check "the address is confirmed" "$ST" "204"
check "  and the date is recorded"  "$(psql_q "SELECT email_verified_at IS NOT NULL FROM users WHERE email = '$EMAIL'")" "t"

ST=$(code -X POST "$B/api/v1/auth/verify-email" -H 'content-type: application/json'  -d "{\"token\":\"$VERIFY\"}")
check "the link is single-use too" "$ST" "400"

# Cross-purpose reuse: a verification token must not reset a password. Both
# are 32 random bytes hashed the same way; only `purpose` separates them.
code -X POST "$B/api/v1/auth/forgot-password" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\"}" >/dev/null
RESET2=$(link_for "$EMAIL" "password_reset")
ST=$(code -X POST "$B/api/v1/auth/verify-email" -H 'content-type: application/json'  -d "{\"token\":\"$RESET2\"}")
check "a reset link cannot verify an email" "$ST" "400"

echo ""
echo "=== 5. the login throttle ==="

THR="ah-throttle-$S@example.com"
code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json'  -d "{\"email\":\"$THR\",\"password\":\"$PW\",\"display_name\":\"Tara Throttle\"}" >/dev/null

# Nine wrong guesses: under the limit, so each is a plain 401.
LAST=""
for i in $(seq 1 9); do LAST=$(login "$THR" "definitely the wrong password"); done
check "nine wrong guesses are still just unauthorised" "$LAST" "401"
check "  and the right password still works" "$(login "$THR" "$PW")" "200"

# A success does NOT clear the failures — otherwise an attacker who happens to
# know one password on the box could reset everyone else's counter.
LAST=""
for i in $(seq 1 3); do LAST=$(login "$THR" "still wrong"); done
check "the twelfth failure is throttled" "$LAST" "429"

# And the lock is real: the CORRECT password is refused too. A throttle that
# lets the right password through is a throttle an attacker walks past on the
# guess that matters.
check "  even with the correct password" "$(login "$THR" "$PW")" "429"

check "  reported as a distinct, branchable code" "$(pyq "d['code']")" "auth.too_many_attempts"

# Another account from the same host is unaffected: the per-address counter is
# the one that bit, and it must not spill onto the neighbours.
OTHER="ah-other-$S@example.com"
code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json'  -d "{\"email\":\"$OTHER\",\"password\":\"$PW\",\"display_name\":\"Otto Other\"}" >/dev/null
check "a different account is unaffected" "$(login "$OTHER" "$PW")" "200"

# Case must not be an escape hatch.
check "changing the case does not reset the counter"  "$(login "$(echo "$THR" | tr '[:lower:]' '[:upper:]')" "$PW")" "429"

echo ""
echo "=== 6. what is written down ==="
check "attempts are recorded, successes and failures alike"  "$(psql_q "SELECT count(*) > 12 FROM login_attempts WHERE email = '$THR'")" "t"
check "  the log cannot be edited by the app role"  "$(psql_q "SELECT count(*) FROM information_schema.table_privileges
              WHERE grantee='gym_app' AND table_name='login_attempts'
                AND privilege_type IN ('UPDATE','DELETE')")" "0"
check "  nor can sent mail"  "$(psql_q "SELECT count(*) FROM information_schema.table_privileges
              WHERE grantee='gym_app' AND table_name='sent_emails'
                AND privilege_type IN ('UPDATE','DELETE')")" "0"
check "  and a spent token is kept, not deleted"  "$(psql_q "SELECT count(*) FROM information_schema.table_privileges
              WHERE grantee='gym_app' AND table_name='auth_tokens'
                AND privilege_type = 'DELETE'")" "0"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
