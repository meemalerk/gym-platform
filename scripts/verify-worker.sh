#!/usr/bin/env bash
# The worker: recurring billing, the outbox, and periodic jobs (ADR-0027).
#
# The claim that matters most, tested more than once from more than one angle:
# **running the billing tick twice bills a member once.** A nightly job that
# issues invoices is precisely the kind of thing that gets run twice — a retry,
# an operator running it by hand, two workers starting together — and
# double-charging somebody is the worst outcome this feature can produce.
#
# Three guards exist and they are tested separately, because a test that only
# runs the job twice would pass even if two of the three were broken:
#
#   §3  the job lease (a second run inside the interval declines)
#   §4  next_charge_on advancing (the same period is not re-selected)
#   §5  the unique index (a direct INSERT of a duplicate is refused by the DB)
#
# The third is the only one that survives a bug in the other two, which is why
# it is asserted against the database rather than through the API.
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
export SERVER_PORT=8107
export SERVER_HOST=127.0.0.1
export JWT_SECRET="dev-only-insecure-secret-change-me-before-any-real-deployment"
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

cargo build --bin server --bin worker 2>&1 | tail -2 || exit 1
./target/debug/server > /tmp/server-worker.log 2>&1 &
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
worker() { ./target/debug/worker --once 2>/dev/null; }

S=$(date +%s%N); PW="correct horse battery staple"

signup() { code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json'  -d "{\"email\":\"$1\",\"password\":\"$PW\",\"display_name\":\"$2\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json; }
uid() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/b.json; }

echo "=== setup: a gym, a monthly plan, a member on it ==="
OWNER_T=$(signup "wk-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T"  -H 'content-type: application/json' -d "{\"name\":\"Worker Gym $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)

MEMBER_T=$(signup "wk-member-$S@example.com" "Mo Member")
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"open_registration":true}' >/dev/null
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $MEMBER_T" >/dev/null
MEMBER=$(uid "$MEMBER_T")

PLAN=$(code -X POST "$B/api/v1/gyms/$GYM/plans" -H "authorization: Bearer $OWNER_T"  -H 'content-type: application/json'  -d '{"name":"Monthly","price_minor":4500,"currency":"GBP","interval":"monthly","grants":["gym_access"]}' >/dev/null; pyq "d['id']")

# Started ~5 weeks ago: exactly ONE monthly period has come due, which keeps
# the assertions below unambiguous. Multi-period catch-up gets its own section.
STARTED=$("$PY" -c "import datetime;print(datetime.date.today() - datetime.timedelta(days=35))")
SUB=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T"  -H 'content-type: application/json'  -d "{\"member_id\":\"$MEMBER\",\"plan_id\":\"$PLAN\",\"started_on\":\"$STARTED\"}" >/dev/null; pyq "d['subscription']['id']")

check "the subscription has a charge date in the past"  "$(psql_q "SELECT next_charge_on <= current_date FROM member_subscriptions WHERE id = '$SUB'")" "t"
# Subscribing bills the first period up front — a gym takes the first month
# at signup — so one invoice exists before the tick has run. The tick's job
# is every period AFTER that one.
check "signing up billed the first period" "$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB'")" "1"

echo ""
echo "=== 1. the tick issues what is due ==="
# Clear any lease left by an earlier run so this suite is self-contained.
psql_q "DELETE FROM job_runs WHERE name IN ('billing-tick','overdue-sweep','idle-clients')" >/dev/null
OUT=$(worker)
check "the worker runs" "$([ -n "$OUT" ] && echo ran)" "ran"
check "  a second invoice was issued"  "$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB'")" "2"
check "  both for the plan's price"  "$(psql_q "SELECT DISTINCT amount_minor FROM invoices WHERE subscription_id = '$SUB'")" "4500"
check "  each naming the period it covers"  "$(psql_q "SELECT bool_and(period_start IS NOT NULL AND period_end IS NOT NULL) FROM invoices WHERE subscription_id = '$SUB'")" "t"
check "  with references in the gym's sequence"  "$(psql_q "SELECT bool_and(reference LIKE 'INV-%') FROM invoices WHERE subscription_id = '$SUB'")" "t"
check "  and the charge date is now in the future"  "$(psql_q "SELECT next_charge_on > current_date FROM member_subscriptions WHERE id = '$SUB'")" "t"

echo ""
echo "=== 2. the member can see it ==="
ST=$(code "$B/api/v1/gyms/$GYM/invoices" -H "authorization: Bearer $MEMBER_T")
check "a member reads their own invoices" "$ST" "200"
check "  the new one is there" "$(pyq "any(i['subscription_id'] == '$SUB' for i in d)")" "True"
check "  and it is due, not paid"  "$(pyq "next(i['status']['state'] for i in d if i['subscription_id'] == '$SUB')")" "due"

echo ""
echo "=== 3. the lease stops a second run ==="
# Immediately re-running must do nothing at all — this is what makes a
# crash-restart loop safe.
worker >/dev/null
check "running again right away issues nothing"  "$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB'")" "2"
check "  because the job declined the lease"  "$(psql_q "SELECT last_outcome LIKE 'issued 1%' FROM job_runs WHERE name = 'billing-tick'")" "t"

echo ""
echo "=== 4. and so does the advanced charge date ==="
# Force the lease open. The tick now runs for real a second time, and the ONLY
# thing preventing a double-bill is that next_charge_on has moved past today.
psql_q "UPDATE job_runs SET last_finished_at = now() - interval '2 days' WHERE name = 'billing-tick'" >/dev/null
worker >/dev/null
check "a genuine second run still issues nothing"  "$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB'")" "2"

echo ""
echo "=== 5. the database refuses a duplicate outright ==="
# The guard that survives a bug in the other two. Insert a second invoice for
# the same subscription and period by hand; the unique index must refuse it.
PERIOD=$(psql_q "SELECT period_start FROM invoices WHERE subscription_id = '$SUB' ORDER BY period_start LIMIT 1")
DUP=$(docker exec gym-postgres psql -U gym -d gym -tAc "
  INSERT INTO invoices (id, gym_id, member_id, subscription_id, reference, description,
                        amount_minor, currency, period_start, period_end, issued_on, due_on, status)
  VALUES (gen_random_uuid(), '$GYM', '$MEMBER', '$SUB', 'INV-DUP-1', 'duplicate',
          4500, 'GBP', '$PERIOD', '$PERIOD', current_date, current_date, 'due')" 2>&1 | tr -d '\r')
check "a duplicate period is rejected by the index"  "$(echo "$DUP" | grep -c 'invoices_one_per_subscription_period')" "1"

# But a VOID invoice must not block the corrected re-issue — a gym that
# invoiced in error has to be able to bill the period properly.
psql_q "UPDATE invoices SET status='void', voided_at=now(), voided_by='$MEMBER' WHERE subscription_id='$SUB' AND period_start='$PERIOD'" >/dev/null
REDO=$(docker exec gym-postgres psql -U gym -d gym -tAc "
  INSERT INTO invoices (id, gym_id, member_id, subscription_id, reference, description,
                        amount_minor, currency, period_start, period_end, issued_on, due_on, status)
  VALUES (gen_random_uuid(), '$GYM', '$MEMBER', '$SUB', 'INV-REDO-$S', 'corrected',
          4500, 'GBP', '$PERIOD', '$PERIOD', current_date, current_date, 'due')" 2>&1 | tr -d '\r')
check "  but a voided one does not block the correction" "$(echo "$REDO" | grep -c 'INSERT 0 1')" "1"

echo ""
echo "=== 5b. arrears are caught up in ONE run ==="
# A membership three months behind owes three invoices. Issuing one per
# nightly run would take three nights, and the member would see a partial,
# wrong balance the whole time. The first version of this job did exactly
# that; the suite caught it by finding successive months appearing on
# successive runs instead of together.
ARR_T=$(signup "wk-arrears-$S@example.com" "Ada Arrears")
code -X POST "$B/api/v1/gyms/$GYM/join" -H "authorization: Bearer $ARR_T" >/dev/null
# Shut the door, now that every member this suite needs is in.
#
# It must be here and not up in the setup: this suite joins a SECOND member
# much later for the arrears case, and closing early locked them out — which
# showed up as the arrears section returning empty rather than as a refusal.
#
# Left open, this throwaway gym joins the list a new member is offered at
# sign-up, which is how that list reached 163 rows.
code -X PUT "$B/api/v1/gyms/$GYM/settings/registration" \
  -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
  -d '{"open_registration":false}' >/dev/null
ARR=$(uid "$ARR_T")

LONG_AGO=$("$PY" -c "import datetime;print(datetime.date.today() - datetime.timedelta(days=100))")
SUB2=$(code -X POST "$B/api/v1/gyms/$GYM/subscriptions" -H "authorization: Bearer $OWNER_T"   -H 'content-type: application/json'   -d "{\"member_id\":\"$ARR\",\"plan_id\":\"$PLAN\",\"started_on\":\"$LONG_AGO\"}" >/dev/null; pyq "d['subscription']['id']")

psql_q "DELETE FROM job_runs WHERE name = 'billing-tick'" >/dev/null
worker >/dev/null
check "three months of arrears are settled in one run"   "$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB2'")" "4"
check "  each for a distinct period"   "$(psql_q "SELECT count(DISTINCT period_start) FROM invoices WHERE subscription_id = '$SUB2'")" "4"
check "  and the charge date is now in the future"   "$(psql_q "SELECT next_charge_on > current_date FROM member_subscriptions WHERE id = '$SUB2'")" "t"

psql_q "DELETE FROM job_runs WHERE name = 'billing-tick'" >/dev/null
worker >/dev/null
check "  a further run adds nothing"   "$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB2'")" "4"

echo ""
echo "=== 6. the outbox ==="
check "the tick emitted one event per invoice it issued"  "$(psql_q "SELECT count(*) FROM domain_events WHERE event_type = 'invoice.issued' AND gym_id = '$GYM'")" "4"
check "  and the worker drained it"  "$(psql_q "SELECT count(*) FROM domain_events WHERE event_type = 'invoice.issued' AND gym_id = '$GYM' AND processed_at IS NULL")" "0"
check "  each carrying enough to act on"  "$(psql_q "SELECT bool_and(payload ? 'reference' AND payload ? 'member_id' AND payload ? 'amount') FROM domain_events WHERE event_type='invoice.issued' AND gym_id='$GYM'")" "t"

# The app role may emit but must never consume — otherwise a request handler
# could quietly drain the worker's queue.
check "the app role may insert events"  "$(psql_q "SELECT count(*) FROM information_schema.table_privileges WHERE grantee='gym_app' AND table_name='domain_events' AND privilege_type='INSERT'")" "1"
check "  and may NOT mark them processed"  "$(psql_q "SELECT count(*) FROM information_schema.table_privileges WHERE grantee='gym_app' AND table_name='domain_events' AND privilege_type='UPDATE'")" "0"
check "  nor read them at all"  "$(psql_q "SELECT count(*) FROM information_schema.table_privileges WHERE grantee='gym_app' AND table_name='domain_events' AND privilege_type='SELECT'")" "0"

echo ""
echo "=== 7. overdue is noticed, not stored ==="
# An already-late invoice, INSERTED rather than back-dated.
#
# Updating an existing one is impossible by design: `reject_issued_invoice_edit`
# refuses any edit to an issued invoice ("void it and issue another"), and
# `invoice_due_after_issue` refuses a due date before the issue date. Both
# fired when this test first tried the obvious thing, which is the immutability
# rule working exactly as ADR-0010 intends. So the fixture is a genuinely old
# invoice, not a rewritten new one.
LATE_REF="INV-LATE-$S"
docker exec gym-postgres psql -U gym -d gym -q -c "
  INSERT INTO invoices (id, gym_id, member_id, subscription_id, reference, description,
                        amount_minor, currency, issued_on, due_on, status)
  VALUES (gen_random_uuid(), '$GYM', '$MEMBER', NULL, '$LATE_REF', 'late charge',
          2000, 'GBP', current_date - 20, current_date - 5, 'due')" >/dev/null 2>&1

psql_q "DELETE FROM job_runs WHERE name = 'overdue-sweep'" >/dev/null
worker >/dev/null

check "the overdue invoice was noticed"  "$(psql_q "SELECT count(*) FROM domain_events WHERE event_type='invoice.overdue' AND gym_id='$GYM'")" "1"
check "  the event says how late it is"  "$(psql_q "SELECT (payload ->> 'days_late')::int >= 5 FROM domain_events WHERE event_type='invoice.overdue' AND gym_id='$GYM'")" "t"
# The whole point: noticing must not mutate. "Overdue" stays derived from the
# due date and today, exactly as the invoices migration insists.
check "  and the invoice itself is untouched"  "$(psql_q "SELECT status FROM invoices WHERE reference='$LATE_REF'")" "due"

psql_q "UPDATE job_runs SET last_finished_at = now() - interval '2 days' WHERE name='overdue-sweep'" >/dev/null
worker >/dev/null
check "  and it is not noticed twice"  "$(psql_q "SELECT count(*) FROM domain_events WHERE event_type='invoice.overdue' AND gym_id='$GYM'")" "1"

echo ""
echo "=== 8. cancelling stops the billing ==="
psql_q "UPDATE member_subscriptions SET next_charge_on = current_date - 1 WHERE id='$SUB'" >/dev/null
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/subscriptions/$SUB" -H "authorization: Bearer $OWNER_T")
BEFORE=$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB'")
psql_q "DELETE FROM job_runs WHERE name = 'billing-tick'" >/dev/null
worker >/dev/null
check "a cancelled subscription is not billed again"  "$(psql_q "SELECT count(*) FROM invoices WHERE subscription_id = '$SUB'")" "$BEFORE"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
