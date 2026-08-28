#!/usr/bin/env bash
# The gym operating calendar (ADR-0015), over HTTP.
#
# The claim being tested is one sentence: **an override wins entirely.** Not
# merged with the weekly pattern, not extending it, not layered behind a
# precedence ladder — replaced. ADR-0015 chose that because any merging rule is
# a thing people get wrong at the edges, and §3 is where it is pinned.
#
# The other load-bearing distinction is between "closed" and "not configured".
# A gym that never set up Sunday and a gym that closed for Eid are both shut,
# and telling a member the wrong one of those either turns them away or sends
# them to a locked door.
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
export SERVER_PORT=8111
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
./target/debug/server > /tmp/server-calendar.log 2>&1 &
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

# Fixed dates, so the assertions do not depend on what day the suite runs.
# 2026-09-06 is a Sunday; 2026-09-07 a Monday; 2026-09-09 a Wednesday.
SUN="2026-09-06"; MON="2026-09-07"; TUE="2026-09-08"; WED="2026-09-09"

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
day() { pyq "next(x for x in d['days'] if x['date'] == '$1')$2"; }
# Separate helper for counts: the expressions above are evaluated by PYTHON,
# so `|length` (jq) is a syntax error that reads as an empty result.
dayspans() { pyq "len(next(x for x in d['days'] if x['date'] == '$1')['spans'])"; }

echo "=== setup ==="
OWNER_T=$(signup "cal-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Calendar Gym $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)

MGR_T=$(signup "cal-mgr-$S@example.com" "Mara Manager");     accept_inv "$MGR_T" "$(invite "cal-mgr-$S@example.com" '["owner"]')"
# Staff, but not a manager. Opening hours are a settings question, so this
# account exists to be refused them — since ADR-0036 that is a trainer.
NM_T=$(signup "cal-nm-$S@example.com" "Nina NotManager"); accept_inv "$NM_T" "$(invite "cal-nm-$S@example.com" '["trainer"]')"
T1_T=$(signup "cal-t1-$S@example.com" "Tariq Trainer"); accept_inv "$T1_T" "$(invite "cal-t1-$S@example.com" '["trainer"]')"
M1_T=$(signup "cal-m1-$S@example.com" "Mo Member");     accept_inv "$M1_T" "$(invite "cal-m1-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); M1=$(uid "$M1_T")

echo ""
echo "=== 1. who may set the hours ==="
HOURS='{"spans":[
  {"weekday":1,"opens_at":"06:00:00","closes_at":"10:00:00"},
  {"weekday":1,"opens_at":"16:00:00","closes_at":"22:00:00"},
  {"weekday":2,"opens_at":"06:00:00","closes_at":"22:00:00"},
  {"weekday":3,"opens_at":"06:00:00","closes_at":"22:00:00"}]}'

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/hours" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d "$HOURS")
check "a member may not set opening hours" "$ST" "403"

# Deliberately narrower than can_manage_catalogue: when the building is open is
# a business fact, not a coaching decision.
ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/hours" -H "authorization: Bearer $NM_T" \
  -H 'content-type: application/json' -d "$HOURS")
check "nor may a trainer" "$ST" "403"

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/hours" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "$HOURS")
check "an owner may" "$ST" "200"
check "  and all four spans were stored" "$(pyq "len(d)")" "4"

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/hours" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d '{"spans":[{"weekday":1,"opens_at":"22:00:00","closes_at":"06:00:00"}]}')
check "closing before opening is refused" "$ST" "400"
ST=$(code "$B/api/v1/gyms/$GYM/calendar/hours" -H "authorization: Bearer $OWNER_T")
check "  and the refusal changed nothing" "$(pyq "len(d)")" "4"

echo ""
echo "=== 2. the pattern resolves ==="
ST=$(code "$B/api/v1/gyms/$GYM/calendar?from=$SUN&to=$WED" -H "authorization: Bearer $M1_T")
check "a member may read the calendar" "$ST" "200"
check "  four days come back" "$(pyq "len(d['days'])")" "4"
check "  with the gym's timezone" "$(pyq "d['timezone']")" "UTC"

check "Monday has both shifts" "$(dayspans "$MON")" "2"
check "  in order, earliest first" "$(day "$MON" "['spans'][0]['opens_at']")" "06:00:00"
check "  and the evening one second" "$(day "$MON" "['spans'][1]['opens_at']")" "16:00:00"
check "Tuesday is one long day" "$(dayspans "$TUE")" "1"

# The distinction the `closed_by_override` flag exists for.
check "Sunday is shut" "$(day "$SUN" "['is_open']")" "False"
check "  but NOT by an override — nobody configured it" \
  "$(day "$SUN" "['closed_by_override']")" "False"
check "  so there is no reason to give" "$(day "$SUN" "['reason'] is None")" "True"

echo ""
echo "=== 3. an override wins ENTIRELY ==="
ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/overrides" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"date\":\"$MON\",\"is_closed\":false,\"opens_at\":\"09:00:00\",\"closes_at\":\"13:00:00\",\"reason\":\"Stocktake\"}")
check "special hours can be set" "$ST" "204"

code "$B/api/v1/gyms/$GYM/calendar?from=$SUN&to=$WED" -H "authorization: Bearer $M1_T" >/dev/null
# THE assertion. Monday's pattern is two shifts; the override is one span, and
# the pattern must not leak through around it.
check "Monday is now exactly the override" "$(dayspans "$MON")" "1"
check "  opening when it says" "$(day "$MON" "['spans'][0]['opens_at']")" "09:00:00"
check "  and the 06:00 shift is gone, not merged" \
  "$(day "$MON" "['spans'][0]['opens_at'] != '06:00:00'")" "True"
check "  with the reason attached" "$(day "$MON" "['reason']")" "Stocktake"
check "Tuesday is untouched" "$(day "$TUE" "['spans'][0]['opens_at']")" "06:00:00"

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/overrides" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"date\":\"$TUE\",\"is_closed\":true,\"reason\":\"Public holiday\"}")
check "a date can be closed outright" "$ST" "204"
code "$B/api/v1/gyms/$GYM/calendar?from=$SUN&to=$WED" -H "authorization: Bearer $M1_T" >/dev/null
check "  Tuesday is shut" "$(day "$TUE" "['is_open']")" "False"
check "  and says it was a decision" "$(day "$TUE" "['closed_by_override']")" "True"
check "  naming the reason" "$(day "$TUE" "['reason']")" "Public holiday"

echo ""
echo "=== 4. an override is one per date, and reversible ==="
ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/overrides" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"date\":\"$MON\",\"is_closed\":true,\"reason\":\"Changed our minds\"}")
check "setting the same date again replaces it" "$ST" "204"
code "$B/api/v1/gyms/$GYM/calendar?from=$MON&to=$MON" -H "authorization: Bearer $M1_T" >/dev/null
check "  Monday is now closed, not both" "$(day "$MON" "['is_open']")" "False"
check "  with the new reason" "$(day "$MON" "['reason']")" "Changed our minds"

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/calendar/overrides" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"date\":\"$WED\",\"is_closed\":false}")
check "open-with-no-hours is refused" "$ST" "400"

ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/calendar/overrides/$MON" -H "authorization: Bearer $OWNER_T")
check "an override can be cleared" "$ST" "204"
code "$B/api/v1/gyms/$GYM/calendar?from=$MON&to=$MON" -H "authorization: Bearer $M1_T" >/dev/null
check "  and the weekly pattern comes back" "$(dayspans "$MON")" "2"

ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/calendar/overrides/$WED" -H "authorization: Bearer $OWNER_T")
check "clearing a date with no override is not found" "$ST" "404"

echo ""
echo "=== 5. trainer availability ==="
AVAIL='{"spans":[
  {"weekday":1,"opens_at":"05:00:00","closes_at":"12:00:00"},
  {"weekday":3,"opens_at":"09:00:00","closes_at":"17:00:00"}]}'

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/trainers/$T1/availability" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d "$AVAIL")
check "a trainer sets their OWN availability" "$ST" "200"

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/trainers/$T1/availability" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d "$AVAIL")
check "a member may not set someone else's" "$ST" "403"

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/trainers/$T1/availability" -H "authorization: Bearer $MGR_T" \
  -H 'content-type: application/json' -d "$AVAIL")
check "a second owner may" "$ST" "200"

ST=$(code -X PUT "$B/api/v1/gyms/$GYM/trainers/$M1/availability" -H "authorization: Bearer $MGR_T" \
  -H 'content-type: application/json' -d "$AVAIL")
check "availability for a non-coach is refused" "$ST" "404"

echo ""
echo "=== 6. bookable = availability ∩ open ==="
ST=$(code "$B/api/v1/gyms/$GYM/trainers/$T1/bookable?from=$MON&to=$WED" -H "authorization: Bearer $M1_T")
check "bookable spans resolve" "$ST" "200"

bday() { pyq "next(x for x in d if x['date'] == '$1')$2"; }
bdayspans() { pyq "len(next(x for x in d if x['date'] == '$1')['spans'])"; }

# Available 05:00-12:00; the gym opens at 06:00 and shuts 10:00-16:00. So the
# hour before opening is not bookable, and neither is the midday gap.
check "Monday is clipped to the gym's shifts" "$(bdayspans "$MON")" "1"
check "  starting when the doors open, not when they are free" \
  "$(bday "$MON" "['spans'][0]['opens_at']")" "06:00:00"
check "  and ending when their availability does" \
  "$(bday "$MON" "['spans'][0]['closes_at']")" "10:00:00"

check "Tuesday is closed, so nothing is bookable" "$(bdayspans "$TUE")" "0"
check "Wednesday follows their own window" "$(bday "$WED" "['spans'][0]['opens_at']")" "09:00:00"

echo ""
echo "=== 7. the window is bounded ==="
ST=$(code "$B/api/v1/gyms/$GYM/calendar?from=$WED&to=$MON" -H "authorization: Bearer $M1_T")
check "an inverted window is refused" "$ST" "400"
ST=$(code "$B/api/v1/gyms/$GYM/calendar?from=2026-01-01&to=2030-01-01" -H "authorization: Bearer $M1_T")
check "an absurd window is refused, not served" "$ST" "400"
ST=$(code "$B/api/v1/gyms/$GYM/calendar" -H "authorization: Bearer $M1_T")
check "no window at all defaults to a fortnight" "$(pyq "len(d['days'])")" "14"

echo ""
echo "=== 8. the changes are audited ==="
ST=$(code "$B/api/v1/gyms/$GYM/audit" -H "authorization: Bearer $OWNER_T")
check "setting hours was recorded" "$(pyq "any(e['action'] == 'gym.hours_changed' for e in d)")" "True"
check "closing a day was recorded" "$(pyq "any(e['action'] == 'gym.closure_set' for e in d)")" "True"
check "special hours were recorded" "$(pyq "any(e['action'] == 'gym.special_hours_set' for e in d)")" "True"
check "clearing an override was recorded" \
  "$(pyq "any(e['action'] == 'gym.override_removed' for e in d)")" "True"
check "so was a trainer changing their availability" \
  "$(pyq "any(e['action'] == 'trainer.availability_changed' for e in d)")" "True"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
