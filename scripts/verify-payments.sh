#!/usr/bin/env bash
# Self-service card payment, end to end (ADR-0028).
#
# This is the flow that had no executable proof. A Stripe key is a real
# commercial account, so the whole redirect-pay-return-settle path — the one
# place in the product where money changes hands — was the only significant
# feature never exercised by the suite, in a codebase whose entire discipline
# is executable proof (ADR-0019). The self-hosted gateway exists to close that.
#
# What is under test is NOT the fake bank. It is everything downstream of it,
# all of which is the production path shared with Stripe:
#
#   §3  the invoice settles, once, through `apply_gateway_payment`
#   §4  a resubmitted form does not charge twice
#   §5  a part payment does NOT settle the bill
#   §6  the session token cannot be re-pointed, reused across gyms, or forged
cd "$(dirname "$0")/.." || exit 1

export VTMP="target/verify-tmp"
mkdir -p "$VTMP"

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
export SERVER_PORT=8109
export SERVER_HOST=127.0.0.1
export JWT_SECRET="dev-only-insecure-secret-change-me-before-any-real-deployment"
export PAYMENT_GATEWAY=dummy
export PUBLIC_BASE_URL="http://127.0.0.1:$SERVER_PORT"
export RUST_LOG=warn
B="http://127.0.0.1:$SERVER_PORT"

# Reap leftover servers from earlier suites — and ONLY our own servers.
# Match on the PROCESS, never on the port: an earlier version of this swept a
# port range and took Docker Desktop down with it.
reap_stale_servers() {
  if command -v taskkill >/dev/null 2>&1; then
    taskkill //F //IM server.exe >/dev/null 2>&1 || true
  elif command -v pkill >/dev/null 2>&1; then
    pkill -9 -f "$PWD/target/debug/server" 2>/dev/null || true
  fi
}
reap_stale_servers

cargo build --bin server 2>&1 | tail -2 || exit 1
./target/debug/server > /tmp/server-pay.log 2>&1 &
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
# The card page is HTML, so these read the body directly rather than as JSON.
page() { curl -s -o "$VTMP/page.html" -w "%{http_code}" "$@"; }
body() { cat "$VTMP/page.html"; }

S=$(date +%s%N); PW="correct horse battery staple"; TODAY=$(date +%F)

signup() { code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$1\",\"password\":\"$PW\",\"display_name\":\"$2\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json; }
uid() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/b.json; }
# ADR-0031: no invitations. Open the door and walk in — `member` is what
# the door grants, which is all this suite needs. The address argument is
# kept in the signature so the call sites below still read as a sentence.
join_gym() { local t="$1" g="$3" o="$4"
  code -X PUT "$B/api/v1/gyms/$g/settings/registration" -H "authorization: Bearer $o" \
    -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
  code -X POST "$B/api/v1/gyms/$g/join" -H "authorization: Bearer $t" >/dev/null
  # Shut the door again straight away.
  #
  # Must be INSIDE the helper: the brace used to sit on the join line, so a
  # block appended after it ran once at load time with $g and $o unset — the
  # door stayed open and the gym turned up in a new member's sign-up list.
  code -X PUT "$B/api/v1/gyms/$g/settings/registration" \
    -H "authorization: Bearer $o" -H 'content-type: application/json' \
    -d '{"open_registration":false}' >/dev/null; }

echo "=== setup: a gym, a member, an unpaid invoice ==="
OWNER_T=$(signup "pay-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Pay Gym $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)

MEMBER_T=$(signup "pay-member-$S@example.com" "Mo Member")
join_gym "$MEMBER_T" "pay-member-$S@example.com" "$GYM" "$OWNER_T"
MEMBER=$(uid "$MEMBER_T")

INVOICE=$(code -X POST "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$MEMBER\",\"description\":\"Personal training block\",\"amount_minor\":12000,\"currency\":\"GBP\",\"due_on\":\"$TODAY\"}" >/dev/null; pyq "d['id']")
check "an invoice exists and is due" "$(pyq "d['status']['state']")" "due"

echo ""
echo "=== 1. starting a checkout ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INVOICE/checkout" -H "authorization: Bearer $MEMBER_T" \
  -H 'content-type: application/json' -d '{"return_url":"gymapp://membership"}')
check "a member may start checkout for their own invoice" "$ST" "201"
CHECKOUT_URL=$(pyq "d['checkout_url']")
REF=$(pyq "d['provider_ref']")
check "  it returns an absolute url on this host" \
  "$("$PY" -c "print('$CHECKOUT_URL'.startswith('$B/pay/'))")" "True"
check "  and the gateway's reference for the attempt" \
  "$("$PY" -c "print('$REF'.startswith('dummy_'))")" "True"

# Nothing has changed yet. Starting a checkout is not a payment.
check "  the invoice is still unpaid" \
  "$(psql_q "SELECT status FROM invoices WHERE id = '$INVOICE'")" "due"
check "  and no payment was recorded" \
  "$(psql_q "SELECT count(*) FROM payments WHERE invoice_id = '$INVOICE'")" "0"

TOKEN="${CHECKOUT_URL##*/pay/}"

echo ""
echo "=== 2. the card page ==="
ST=$(page "$CHECKOUT_URL")
check "the page renders" "$ST" "200"
check "  showing the amount to pay" "$([ "$(body | grep -c '£120.00')" -ge 2 ] && echo yes)" "yes"
check "  and what the charge is for" "$(body | grep -c 'Personal training block')" "1"
check "  labelled as a demo, not a real payment" "$(body | grep -c 'Demo payment')" "1"
check "  telling you which card declines" "$(body | grep -c '4000 0000 0000 0002')" "1"

echo ""
echo "=== 3. paying ==="
ST=$(curl -s -o "$VTMP/page.html" -w "%{http_code}" -X POST "$CHECKOUT_URL" \
  --data-urlencode "number=4242 4242 4242 4242" \
  --data-urlencode "expiry=12/30" \
  --data-urlencode "cvc=123")
check "an approved card redirects back to the app" "$ST" "303"

check "  the invoice is settled" \
  "$(psql_q "SELECT status FROM invoices WHERE id = '$INVOICE'")" "paid"
check "  a payment was recorded" \
  "$(psql_q "SELECT count(*) FROM payments WHERE invoice_id = '$INVOICE'")" "1"
check "  for the full amount" \
  "$(psql_q "SELECT amount_minor FROM payments WHERE invoice_id = '$INVOICE'")" "12000"
# The row must be unmistakable. A demo payment that looks like a real one in
# the money table is a liability.
check "  under the dummy provider, not stripe" \
  "$(psql_q "SELECT provider FROM payments WHERE invoice_id = '$INVOICE'")" "dummy"
check "  saying plainly that no money moved" \
  "$(psql_q "SELECT note LIKE '%no money moved%' FROM payments WHERE invoice_id = '$INVOICE'")" "t"
check "  keyed on the session reference" \
  "$(psql_q "SELECT provider_ref = '$REF' FROM payments WHERE invoice_id = '$INVOICE'")" "t"

ST=$(code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $MEMBER_T")
check "  and the member sees it as paid" \
  "$(pyq "next(i['status']['state'] for i in d if i['id'] == '$INVOICE')")" "paid"

echo ""
echo "=== 4. submitting twice does not charge twice ==="
# A refreshed browser, a double-tapped button, a retried request. The session
# reference is the idempotency key and this is what it is for.
curl -s -o /dev/null -X POST "$CHECKOUT_URL" \
  --data-urlencode "number=4242 4242 4242 4242" --data-urlencode "expiry=12/30" --data-urlencode "cvc=123"
check "still exactly one payment" \
  "$(psql_q "SELECT count(*) FROM payments WHERE invoice_id = '$INVOICE'")" "1"
check "  and the invoice is not paid twice over" \
  "$(psql_q "SELECT sum(amount_minor) FROM payments WHERE invoice_id = '$INVOICE'")" "12000"

echo ""
echo "=== 5. the unhappy paths ==="
INV2=$(code -X POST "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$MEMBER\",\"description\":\"Second charge\",\"amount_minor\":5000,\"currency\":\"GBP\",\"due_on\":\"$TODAY\"}" >/dev/null; pyq "d['id']")
URL2=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV2/checkout" -H "authorization: Bearer $MEMBER_T" \
  -H 'content-type: application/json' -d '{"return_url":"gymapp://membership"}' >/dev/null; pyq "d['checkout_url']")

ST=$(curl -s -o "$VTMP/page.html" -w "%{http_code}" -X POST "$URL2" \
  --data-urlencode "number=4000 0000 0000 0002" --data-urlencode "expiry=12/30" --data-urlencode "cvc=123")
check "a declined card stays on the page" "$ST" "200"
check "  and says so" "$([ "$(body | grep -c 'declined')" -ge 1 ] && echo yes)" "yes"
check "  changing nothing" "$(psql_q "SELECT status FROM invoices WHERE id = '$INV2'")" "due"

ST=$(curl -s -o "$VTMP/page.html" -w "%{http_code}" -X POST "$URL2" \
  --data-urlencode "number=4242" --data-urlencode "expiry=12/30" --data-urlencode "cvc=123")
check "a mistyped card is distinguished from a refusal" "$(body | grep -c 'Check the card number')" "1"
check "  and still changes nothing" "$(psql_q "SELECT status FROM invoices WHERE id = '$INV2'")" "due"

# An already-paid invoice must not be payable again.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INVOICE/checkout" -H "authorization: Bearer $MEMBER_T" \
  -H 'content-type: application/json' -d '{"return_url":"gymapp://membership"}')
check "a settled invoice cannot start another checkout" "$ST" "409"

echo ""
echo "=== 6. the session token is the only trust ==="
ST=$(page "$B/pay/not-a-real-token")
check "a forged token is refused" "$ST" "400"
check "  without explaining why" "$(body | grep -c 'no longer valid')" "1"

# An access token is signed with the SAME key. The purpose claim is what stops
# it being spent — the same guard the entry pass uses.
ST=$(page "$B/pay/$MEMBER_T")
check "an access token is not a payment session" "$ST" "400"

# Somebody else's invoice: the checkout call itself must refuse, so no token
# for it ever exists.
OTHER_T=$(signup "pay-other-$S@example.com" "Otto Other")
join_gym "$OTHER_T" "pay-other-$S@example.com" "$GYM" "$OWNER_T"
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV2/checkout" -H "authorization: Bearer $OTHER_T" \
  -H 'content-type: application/json' -d '{"return_url":"gymapp://membership"}')
check "another member cannot start checkout for your invoice" "$ST" "404"

echo ""
echo "=== 7. a part payment does not settle the bill ==="
# The rule predates this gateway and must survive it: money received is not the
# same as an invoice settled.
INV3=$(code -X POST "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$MEMBER\",\"description\":\"Part paid\",\"amount_minor\":10000,\"currency\":\"GBP\",\"due_on\":\"$TODAY\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV3/payments" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"amount_minor\":4000,\"provider\":\"cash\",\"received_on\":\"$TODAY\"}" >/dev/null
check "a part payment leaves the invoice due" \
  "$(psql_q "SELECT status FROM invoices WHERE id = '$INV3'")" "due"

# The checkout must then be for the REMAINDER, not the original total.
code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV3/checkout" -H "authorization: Bearer $MEMBER_T" \
  -H 'content-type: application/json' -d '{"return_url":"gymapp://membership"}' >/dev/null
URL3=$(pyq "d['checkout_url']")
page "$URL3" >/dev/null
# The remainder, not the original total — a checkout that re-charged the full
# amount after a part payment would overcharge, silently.
check "  and the card page charges only the balance" "$([ "$(body | grep -c '£60.00')" -ge 2 ] && echo yes)" "yes"
check "  and never the original total" "$(body | grep -c '£100.00')" "0"

curl -s -o /dev/null -X POST "$URL3" \
  --data-urlencode "number=4242 4242 4242 4242" --data-urlencode "expiry=12/30" --data-urlencode "cvc=123"
check "  paying the balance settles it" \
  "$(psql_q "SELECT status FROM invoices WHERE id = '$INV3'")" "paid"
check "  with both payments on the record" \
  "$(psql_q "SELECT count(*) FROM payments WHERE invoice_id = '$INV3'")" "2"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
