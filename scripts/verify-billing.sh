#!/usr/bin/env bash
# Billing, end to end over HTTP.
#
# The rules worth proving are the ones an argument with a member turns on:
# money is managed by managers, a member sees only their own invoices, an
# issued invoice is never rewritten, "overdue" is a date passing rather than a
# stored state, and a part-payment does not settle a bill.
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
export SERVER_PORT=8094
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
./target/debug/server > /tmp/server-billing.log 2>&1 &
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
TODAY=$(date +%F)
YESTERDAY=$(date -d "-1 day" +%F)
LONG_AGO=$(date -d "-30 days" +%F)

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
OWNER_T=$(signup "bl-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Billing Box"}' >/dev/null; jget "['id']" < $VTMP/b.json)
# Staff, but not a manager: prices are gym management, not coaching.
NM_T=$(signup "bl-nm-$S@example.com" "Nina NotManager"); accept "$NM_T" "$(invite "bl-nm-$S@example.com" '["trainer"]')"
M1_T=$(signup "bl-m1-$S@example.com" "Mo Member");   accept "$M1_T" "$(invite "bl-m1-$S@example.com" '["member"]')"
M2_T=$(signup "bl-m2-$S@example.com" "Mia Member");  accept "$M2_T" "$(invite "bl-m2-$S@example.com" '["member"]')"
M1=$(uid "$M1_T"); M2=$(uid "$M2_T")
OUTSIDER_T=$(signup "bl-out-$S@example.com" "Otto Outsider")

echo ""
echo "=== 1. plans: managers write, everyone reads ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Coaching","description":"Open gym plus a coach","price_minor":12000,"currency":"GBP","interval":"monthly","grants":["gym_access","coached_programming"]}')
check "owner creates a plan -> 201" "$ST" "201"
PLAN=$(pyq "d['id']")
check "  money is pre-formatted once, centrally" "$(pyq "d['price_label']")" "£120.00"
# Ladder order, not alphabetical and not however the request happened to list
# them — so two gyms selling the same thing describe it the same way.
check "  and the plan says what it confers" "$(pyq "','.join(d['grants'])")" "gym_access,coached_programming"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Nothing","price_minor":500,"currency":"GBP","interval":"monthly","grants":[]}')
check "a plan that grants nothing is refused -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $NM_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Sneaky","price_minor":1,"currency":"GBP","interval":"monthly","grants":["gym_access"]}')
check "a trainer cannot set prices -> 403" "$ST" "403"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Free","price_minor":0,"currency":"GBP","interval":"monthly","grants":["gym_access"]}')
check "a member cannot set prices -> 403" "$ST" "403"

ST=$(code "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $M1_T")
check "a member CAN read what the gym charges -> 200" "$ST" "200"

ST=$(code "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OUTSIDER_T")
check "an outsider gets 404, never a price list" "$ST" "404"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Silly","price_minor":99999999,"currency":"GBP","interval":"monthly","grants":["gym_access"]}')
check "a slipped decimal is refused -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Bad money","price_minor":100,"currency":"pounds","interval":"monthly","grants":["gym_access"]}')
check "a non-ISO currency is refused -> 400" "$ST" "400"

echo ""
echo "=== 2. subscribing bills immediately ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$M1\",\"plan_id\":\"$PLAN\",\"started_on\":\"$TODAY\"}")
check "owner subscribes a member -> 201" "$ST" "201"
# Subscribing creates TWO things and the response now says so. It used to
# return the invoice alone, which meant a caller could not reference the
# subscription it had just made — found while building the nightly billing
# tick, which needs exactly that reference.
check "  the response carries the subscription" "$(pyq "'subscription' in d")" "True"
check "  and the first invoice" "$(pyq "'invoice' in d")" "True"
SUB1=$(pyq "d['subscription']['id']")
INV1=$(pyq "d['invoice']['id']")
check "  the invoice is at the plan's price" "$(pyq "d['invoice']['amount_minor']")" "12000"
check "  numbered per gym and year" "$(pyq "d['invoice']['reference'].startswith('INV-')")" "True"
check "  and the invoice names the subscription that produced it"   "$(pyq "d['invoice']['subscription_id'] == d['subscription']['id']")" "True"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$M1\",\"plan_id\":\"$PLAN\",\"started_on\":\"$TODAY\"}")
check "subscribing the same member twice -> 409" "$ST" "409"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$(uid "$OUTSIDER_T")\",\"plan_id\":\"$PLAN\",\"started_on\":\"$TODAY\"}")
check "billing a non-member -> 404" "$ST" "404"

echo ""
echo "=== 3. a member sees their own money and nobody else's ==="
code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $M1_T" >/dev/null
check "their own invoice is visible" "$(pyq "len(d)")" "1"

code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $M2_T" >/dev/null
check "another member sees none of it" "$(pyq "len(d)")" "0"

code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" >/dev/null
check "the owner sees the gym's" "$(pyq "len(d)")" "1"

ST=$(code "$B/api/v1/gyms/$GYM/invoices/$INV1/payments" -H "authorization: Bearer $M2_T")
check "another member cannot read the receipt -> 404" "$ST" "404"

ST=$(code "$B/api/v1/gyms/$GYM/invoices/$INV1/payments" -H "authorization: Bearer $M1_T")
check "the member whose invoice it is can -> 200" "$ST" "200"

echo ""
echo "=== 4. overdue is a date passing, not a stored state ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$M2\",\"description\":\"Drop-in\",\"amount_minor\":1500,\"currency\":\"GBP\",\"due_on\":\"$LONG_AGO\"}")
check "a manual invoice -> 201" "$ST" "201"
INV2=$(pyq "d['id']")
# due_on is clamped to today at issue: an invoice cannot fall due before it exists.
check "  back-dated due dates clamp to today, not the past" "$(pyq "d['is_overdue']")" "False"

code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" >/dev/null
check "  no invoice is overdue on the day it is issued" \
  "$(pyq "sum(1 for i in d if i['is_overdue'])")" "0"

echo ""
echo "=== 5. payment settles only when it covers the bill ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV1/payments" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"amount_minor\":4000,\"provider\":\"cash\",\"received_on\":\"$TODAY\"}")
check "part payment recorded -> 201" "$ST" "201"

code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" >/dev/null
check "  the invoice is still due after a part payment" \
  "$(pyq "[i for i in d if i['id']=='$INV1'][0]['status']['state']")" "due"
check "  and the running total is visible" \
  "$(pyq "[i for i in d if i['id']=='$INV1'][0]['paid_minor']")" "4000"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV1/payments" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"amount_minor\":8000,\"provider\":\"card_terminal\",\"received_on\":\"$TODAY\"}")
check "the balance recorded -> 201" "$ST" "201"

code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" >/dev/null
check "  now it is paid, in the same transaction" \
  "$(pyq "[i for i in d if i['id']=='$INV1'][0]['status']['state']")" "paid"
check "  and a paid invoice is never overdue" \
  "$(pyq "[i for i in d if i['id']=='$INV1'][0]['is_overdue']")" "False"

code "$B/api/v1/gyms/$GYM/invoices/$INV1/payments" -H "authorization: Bearer $OWNER_T" >/dev/null
check "  both payments are on the record" "$(pyq "len(d)")" "2"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV1/payments" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"amount_minor\":100,\"provider\":\"cash\",\"received_on\":\"$TODAY\"}")
check "a member cannot record their own payment -> 403" "$ST" "403"

echo ""
echo "=== 6. an issued invoice is never rewritten ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV1/void" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"reason":"changed my mind"}')
check "a PAID invoice cannot be voided -> 400 (refund instead)" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV2/void" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"reason":"issued in error"}')
check "an unpaid invoice can be voided -> 200" "$ST" "200"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV2/void" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{}')
check "voiding twice -> 400" "$ST" "400"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV2/payments" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"amount_minor\":1500,\"provider\":\"cash\",\"received_on\":\"$TODAY\"}")
check "a voided invoice takes no payment -> 400" "$ST" "400"

echo ""
echo "=== 7. a refund is another row, never an edit ==="
ST=$(code -X POST "$B/api/v1/gyms/$GYM/invoices/$INV1/payments" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"amount_minor\":-2000,\"provider\":\"cash\",\"received_on\":\"$TODAY\",\"note\":\"goodwill\"}")
check "a refund is a negative payment -> 201" "$ST" "201"

code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $OWNER_T" >/dev/null
check "  and the total received drops accordingly" \
  "$(pyq "[i for i in d if i['id']=='$INV1'][0]['paid_minor']")" "10000"

echo ""
echo "=== 8. archiving a plan keeps its members ==="
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/plans/$PLAN" -H "authorization: Bearer $OWNER_T")
check "owner archives the plan -> 204" "$ST" "204"

ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/plans/$PLAN" -H "authorization: Bearer $OWNER_T")
check "archiving twice -> 409" "$ST" "409"

ST=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$M2\",\"plan_id\":\"$PLAN\",\"started_on\":\"$TODAY\"}")
check "an archived plan takes no new members -> 400" "$ST" "400"

code "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T" >/dev/null
check "  the existing subscriber keeps their subscription" \
  "$(pyq "sum(1 for s in d if s['is_active'])")" "1"

echo ""
echo "=== 9. invoice numbers cannot collide under concurrency ==="
# The first implementation read max(reference)+1 outside the insert, so two
# managers issuing at the same moment computed the same number and the second
# lost to the unique index. Fire ten at once and demand ten distinct numbers.
CONC_PIDS=""
for i in $(seq 1 10); do
  curl -s -o "$VTMP/conc-$i.json" -X POST "$B/api/v1/gyms/$GYM/invoices" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d "{\"member_id\":\"$M2\",\"description\":\"Concurrent $i\",\"amount_minor\":100,\"currency\":\"GBP\",\"due_on\":\"$TODAY\"}" &
  CONC_PIDS="$CONC_PIDS $!"
done
# Only these ten — a bare `wait` also waits on the test server started above,
# which never exits, and the suite hangs forever.
wait $CONC_PIDS

# Each response is parsed from its own file: curl writes a body with no
# trailing newline, so `cat`-ing them together yields one unparseable line.
REFS=$("$PY" -c "
import glob, json
refs = []
for path in sorted(glob.glob('$VTMP/conc-*.json')):
    try:
        with open(path, encoding='utf-8') as f:
            ref = json.load(f).get('reference')
        if ref:
            refs.append(ref)
    except Exception:
        pass
print(len(refs), len(set(refs)))
")
check "ten simultaneous invoices all succeed" "$(echo "$REFS" | cut -d' ' -f1)" "10"
check "  and every reference is distinct" "$(echo "$REFS" | cut -d' ' -f2)" "10"
rm -f "$VTMP"/conc-*.json

echo ""
echo ""
echo "=== 9b. a member ends their own membership, and steps down to solo ==="
# Cancelling used to be managers-only, which left the product able to SELL a
# membership self-service but not to stop one: the only way off a plan was to
# catch a manager. That is not a billing safeguard, it is a retention tactic
# enforced by a missing button.
#
# The money is unchanged -- access still runs to the end of the period already
# paid for, and no refund is invented.
#
# A fresh pair of plans: section 8 archived $PLAN, and an archived plan sells
# to nobody.
COACHED=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Coaching II","price_minor":12000,"currency":"GBP","interval":"monthly","grants":["gym_access","coached_programming"]}' >/dev/null; pyq "d['id']")
SOLO=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"name":"Open Gym","price_minor":4900,"currency":"GBP","interval":"monthly","grants":["gym_access"]}' >/dev/null; pyq "d['id']")

# M2 signs THEMSELVES up. Already allowed -- and the asymmetry with cancelling
# is the whole point of this section.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $M2_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$M2\",\"plan_id\":\"$COACHED\",\"started_on\":\"$TODAY\"}")
check "a member may subscribe themselves" "$ST" "201"
M2SUB=$(pyq "d['subscription']['id']")

# Coaching is bought, so it is held.
code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $M2_T" >/dev/null
check "  which buys coached programming" "$(pyq "any(e['feature']=='coached_programming' for e in d['held'])")" "True"

# Somebody else's subscription is not found, rather than forbidden: whether a
# given id exists is not a thing to confirm to a stranger.
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/subscriptions/$M2SUB" -H "authorization: Bearer $M1_T")
check "another member cannot cancel it -> 404" "$ST" "404"

ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/subscriptions/$M2SUB" -H "authorization: Bearer $M2_T")
check "the holder may cancel it themselves -> 200" "$ST" "200"
check "  and it carries when access actually ends" "$(pyq "d['status']['state']=='cancelled' and d['status']['ends_on'] is not None")" "True"

# 400, not 409. `MemberSubscription::cancel` reports "already cancelled" as a
# DomainError, while the route's OpenAPI block advertises 409 -- a pre-existing
# disagreement between the doc and the code, pinned here as it actually is
# rather than quietly changed: the mapping is shared with the billing worker,
# so moving it is a change to make deliberately, not in passing.
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/subscriptions/$M2SUB" -H "authorization: Bearer $M2_T")
check "  cancelling twice is refused" "$ST" "400"

# Cancelling is NOT leaving the gym: the membership capacity is untouched, so
# "coached -> solo" is this cancel followed by a plan granting gym access
# alone. That is the whole downgrade, with no manager in the loop.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $M2_T" \
  -H 'content-type: application/json' \
  -d "{\"member_id\":\"$M2\",\"plan_id\":\"$SOLO\",\"started_on\":\"$TODAY\"}")
check "  and they step straight down to a solo plan" "$ST" "201"
code "$B/api/v1/gyms/$GYM/entitlements/me" -H "authorization: Bearer $M2_T" >/dev/null
check "  the door still opens" "$(pyq "any(e['feature']=='gym_access' for e in d['held'])")" "True"
check "  and it is the solo plan that says so" "$(pyq "next(e['because'] for e in d['held'] if e['feature']=='gym_access')")" "Your Open Gym membership"

# A manager can still cancel for somebody -- the right was widened, not moved.
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/subscriptions/$SUB1" -H "authorization: Bearer $OWNER_T")
check "a manager may still cancel on a member's behalf -> 200" "$ST" "200"

echo "=== 10. the money trail is audited ==="
code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T" >/dev/null
for action in plan.created subscription.created invoice.issued payment.recorded invoice.voided plan.archived; do
  check "  $action is on the audit trail" \
    "$(pyq "any(e['action']=='$action' for e in d)")" "True"
done

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
