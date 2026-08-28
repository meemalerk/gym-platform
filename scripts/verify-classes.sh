#!/usr/bin/env bash
# Group classes and bookings, over HTTP.
#
# Three claims carry this feature and each has a section below:
#
#   1. A class is a WEEKLY SLOT and a booking is a place in ONE SITTING of it.
#      Nothing stores occurrences, so the timetable read has to derive them —
#      and a booking for the wrong weekday has to be refused rather than
#      quietly held forever (§3, §4).
#   2. Capacity is real. Full means full, and the refusal says so (§5).
#   3. Who may do what: managers publish the timetable, anybody in the gym reads
#      it, members book only for themselves, and a roster is for the class's own
#      instructor (§2, §6, §7).
#
# The clock cases (a sitting that has already started) are proved in the domain
# unit tests, where a time can be chosen; here the suite would have to wait.
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
export SERVER_PORT=8112
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
./target/debug/server > /tmp/server-classes.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for i in $(seq 1 40); do curl -fsS "$B/health" >/dev/null 2>&1 && break; sleep 0.5; done

PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then PASS=$((PASS+1));
          else echo "  FAIL  $1 — got '$2' want '$3'"; FAIL=$((FAIL+1)); fi; }
jget() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.loads(sys.stdin.buffer.read().decode('utf-8'));print(eval('d'+sys.argv[1]))" "$1" 2>/dev/null; }
pyq() { "$PY" -c "import json,sys;sys.stdout.reconfigure(encoding='utf-8');d=json.load(open('$VTMP/b.json', encoding='utf-8'));print(eval(sys.argv[1]))" "$1" 2>/dev/null; }
code() { curl -s -o $VTMP/b.json -w "%{http_code}" "$@"; }
uuid() { "$PY" -c "import uuid;print(uuid.uuid4())"; }

S=$(date +%s%N); PW="correct horse battery staple"

# Fixed FUTURE dates so nothing here depends on the day the suite runs, and no
# sitting has already started. 2027-03-01 is a Monday.
MON="2027-03-01"; TUE="2027-03-02"; WINDOW_FROM="2027-03-01"; WINDOW_TO="2027-03-07"

signup() { code -X POST "$B/api/v1/auth/sign-up" -H 'content-type: application/json' \
  -d "{\"email\":\"$1\",\"password\":\"$PW\",\"display_name\":\"$2\"}" >/dev/null; jget "['access_token']" < $VTMP/b.json; }
uidof() { code "$B/api/v1/me" -H "authorization: Bearer $1" >/dev/null; jget "['user']['id']" < $VTMP/b.json; }
join_as() {
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
  code -X PUT "$B/api/v1/gyms/$GYM/members/$(uidof "$1")/capacities" \
    -H "authorization: Bearer $OWNER_T" -H 'content-type: application/json' \
    -d "{\"capacities\":$2}" >/dev/null; }

mkclass() { # mkclass <token> <name> <weekday> <start> <cap> <instructor>
  code -X POST "$B/api/v1/gyms/$GYM/classes" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' \
    -d "{\"name\":\"$2\",\"instructor_id\":\"$6\",\"weekday\":$3,\"starts_at\":\"$4\",\"duration_minutes\":45,\"capacity\":$5}"; }

book() { # book <token> <class> <date>
  code -X POST "$B/api/v1/gyms/$GYM/classes/$2/bookings" -H "authorization: Bearer $1" \
    -H 'content-type: application/json' -d "{\"id\":\"$(uuid)\",\"on_date\":\"$3\"}"; }

echo "=== setup: a gym, two trainers, three members ==="
OWNER_T=$(signup "cls-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Classes Gym $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)

# Staff, but not a manager — the case that says publishing a timetable is gym
# management rather than a coaching right. Since ADR-0036 that is a trainer.
NM_T=$(signup "cls-nm-$S@example.com" "Nina NotManager"); join_as "$NM_T" '["trainer"]'
T1_T=$(signup "cls-t1-$S@example.com" "Tariq Trainer"); join_as "$T1_T" '["trainer"]'
T2_T=$(signup "cls-t2-$S@example.com" "Yara Yoga");     join_as "$T2_T" '["trainer"]'
M1_T=$(signup "cls-m1-$S@example.com" "Mo Member");     join_as "$M1_T" '["member"]'
M2_T=$(signup "cls-m2-$S@example.com" "Mia Member");    join_as "$M2_T" '["member"]'
M3_T=$(signup "cls-m3-$S@example.com" "Sam Member");    join_as "$M3_T" '["member"]'
T1=$(uidof "$T1_T"); T2=$(uidof "$T2_T"); M1=$(uidof "$M1_T"); M2=$(uidof "$M2_T")

echo ""
echo "=== 1. putting a class on the timetable ==="
ST=$(mkclass "$OWNER_T" "Zumba" 1 "18:00:00" 20 "$T1")
check "an owner may add a class" "$ST" "201"
ZUMBA=$(pyq "d['id']")
check "  and it comes back with the weekday spelled out" "$(pyq "d['weekday_name']")" "Monday"

ST=$(mkclass "$OWNER_T" "Zumba" 1 "18:00:00" 20 "$T1")
check "the same class at the same time is refused" "$ST" "409"

# A different class in the same slot is normal — studio and main floor.
ST=$(mkclass "$OWNER_T" "Pilates" 1 "18:00:00" 12 "$T2")
check "a DIFFERENT class in the same slot is fine" "$ST" "201"
PILATES=$(pyq "d['id']")

ST=$(mkclass "$OWNER_T" "Broken" 7 "18:00:00" 20 "$T1")
check "weekday 7 is refused" "$ST" "400"
ST=$(mkclass "$OWNER_T" "Broken" 1 "18:00:00" 0 "$T1")
check "capacity 0 is refused" "$ST" "400"
ST=$(mkclass "$OWNER_T" "  " 1 "18:00:00" 20 "$T1")
check "a blank name is refused" "$ST" "400"

echo ""
echo "=== 2. who may publish the timetable ==="
# Managing the timetable is gym management, not coaching: a trainer teaches a
# class, they do not decide the gym runs one on Mondays.
ST=$(mkclass "$T1_T" "Trainer's Idea" 3 "10:00:00" 10 "$T1")
check "a trainer may NOT add a class" "$ST" "403"
ST=$(mkclass "$M1_T" "Member's Idea" 3 "11:00:00" 10 "$T1")
check "a member may NOT add a class" "$ST" "403"
ST=$(mkclass "$NM_T" "Trainer Class" 5 "12:00:00" 10 "$T1")
check "a trainer may NOT either (it is not a coaching right)" "$ST" "403"

echo ""
echo "=== 3. the timetable derives sittings from the weekly slot ==="
ST=$(code "$B/api/v1/gyms/$GYM/classes?from=$WINDOW_FROM&to=$WINDOW_TO" -H "authorization: Bearer $M1_T")
check "a member may read the timetable" "$ST" "200"
# One week, two Monday classes -> exactly two rows, both dated the Monday.
check "  one week yields one sitting per class" "$(pyq "len(d)")" "2"
check "  both dated the Monday in the window" "$(pyq "sorted({x['on_date'] for x in d})")" "['$MON']"
check "  occupancy starts empty" "$(pyq "d[0]['booked']")" "0"
check "  places_left is capacity" "$(pyq "max(x['places_left'] for x in d)")" "20"

# Two weeks -> each class appears twice. This is the whole point of not storing
# occurrences: the second week needs no rows to exist.
code "$B/api/v1/gyms/$GYM/classes?from=$WINDOW_FROM&to=2027-03-14" -H "authorization: Bearer $M1_T" >/dev/null
check "a two-week window yields two sittings per class" "$(pyq "len(d)")" "4"

ST=$(code "$B/api/v1/gyms/$GYM/classes?from=2027-03-07&to=2027-03-01" -H "authorization: Bearer $M1_T")
check "a backwards window is refused" "$ST" "400"
ST=$(code "$B/api/v1/gyms/$GYM/classes?from=2027-03-01&to=2027-12-01" -H "authorization: Bearer $M1_T")
check "an absurdly wide window is refused" "$ST" "400"

echo ""
echo "=== 4. a booking is for one sitting, on the class's own weekday ==="
ST=$(book "$M1_T" "$ZUMBA" "$MON")
check "a member books a place" "$ST" "201"
BOOKING=$(pyq "d['id']")

ST=$(book "$M1_T" "$ZUMBA" "$TUE")
check "the Tuesday of a Monday class is refused" "$ST" "400"
check "  and it says which day it runs" \
  "$(pyq "'Monday' in d['detail'] and 'Tuesday' in d['detail']")" "True"

ST=$(book "$M1_T" "$ZUMBA" "$MON")
check "the same member cannot take a second place" "$ST" "409"

# A replayed id is the SAME booking (ADR-0008), not a second place.
RID=$(uuid)
code -X POST "$B/api/v1/gyms/$GYM/classes/$PILATES/bookings" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$RID\",\"on_date\":\"$MON\"}" >/dev/null
ST=$(code -X POST "$B/api/v1/gyms/$GYM/classes/$PILATES/bookings" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d "{\"id\":\"$RID\",\"on_date\":\"$MON\"}")
check "replaying the same booking id is a no-op, not a conflict" "$ST" "201"

code "$B/api/v1/gyms/$GYM/classes?from=$WINDOW_FROM&to=$WINDOW_TO" -H "authorization: Bearer $M1_T" >/dev/null
check "occupancy reflects the booking" \
  "$(pyq "next(x['booked'] for x in d if x['class_id']=='$ZUMBA')")" "1"
check "  and the caller is told it is theirs" \
  "$(pyq "next(x['booked_by_me'] for x in d if x['class_id']=='$ZUMBA')")" "True"
code "$B/api/v1/gyms/$GYM/classes?from=$WINDOW_FROM&to=$WINDOW_TO" -H "authorization: Bearer $M2_T" >/dev/null
check "  somebody else sees the count but not a place of their own" \
  "$(pyq "next((x['booked'], x['booked_by_me']) for x in d if x['class_id']=='$ZUMBA')")" "(1, False)"

echo ""
echo "=== 5. capacity is real ==="
ST=$(mkclass "$OWNER_T" "Tiny Class" 1 "06:00:00" 1 "$T1")
check "a one-place class is legal" "$ST" "201"
TINY=$(pyq "d['id']")
ST=$(book "$M1_T" "$TINY" "$MON")
check "  the first member gets in" "$ST" "201"
ST=$(book "$M2_T" "$TINY" "$MON")
check "  the second is refused" "$ST" "409"
check "  and told it is full, with the number" \
  "$(pyq "'full' in d['detail'] and '1' in d['detail']")" "True"

code "$B/api/v1/gyms/$GYM/classes?from=$WINDOW_FROM&to=$WINDOW_TO" -H "authorization: Bearer $M2_T" >/dev/null
check "  the row says FULL with no places left" \
  "$(pyq "next((x['is_full'], x['places_left']) for x in d if x['class_id']=='$TINY')")" "(True, 0)"

echo ""
echo "=== 6. giving a place back ==="
# A cancelled place frees the slot for somebody else, which is the entire
# reason cancellation is a timestamp rather than a delete.
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/class-bookings/$BOOKING" -H "authorization: Bearer $M2_T")
check "a member may NOT cancel somebody else's place" "$ST" "404"
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/class-bookings/$BOOKING" -H "authorization: Bearer $M1_T")
check "the holder may cancel their own" "$ST" "200"
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/class-bookings/$BOOKING" -H "authorization: Bearer $M1_T")
check "  twice is refused" "$ST" "409"

code "$B/api/v1/gyms/$GYM/classes?from=$WINDOW_FROM&to=$WINDOW_TO" -H "authorization: Bearer $M1_T" >/dev/null
check "  the place is back on the timetable" \
  "$(pyq "next((x['booked'], x['booked_by_me']) for x in d if x['class_id']=='$ZUMBA')")" "(0, False)"

# And re-bookable, which the partial unique index is what allows.
ST=$(book "$M1_T" "$ZUMBA" "$MON")
check "  and the same member may book it again" "$ST" "201"

echo ""
echo "=== 7. the roster is for the class's own instructor ==="
ST=$(code "$B/api/v1/gyms/$GYM/classes/$ZUMBA/roster?on_date=$MON" -H "authorization: Bearer $T1_T")
check "the instructor sees their own roster" "$ST" "200"
check "  with the member on it" "$(pyq "len(d)")" "1"
check "  by name" "$(pyq "d[0]['member_name']")" "Mo Member"

ST=$(code "$B/api/v1/gyms/$GYM/classes/$ZUMBA/roster?on_date=$MON" -H "authorization: Bearer $T2_T")
check "another trainer may NOT read it" "$ST" "403"
ST=$(code "$B/api/v1/gyms/$GYM/classes/$ZUMBA/roster?on_date=$MON" -H "authorization: Bearer $M1_T")
check "a member may NOT read it" "$ST" "403"
ST=$(code "$B/api/v1/gyms/$GYM/classes/$ZUMBA/roster?on_date=$MON" -H "authorization: Bearer $OWNER_T")
check "a manager may" "$ST" "200"

echo ""
echo "=== 8. taking a class off the timetable ==="
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/classes/$TINY" -H "authorization: Bearer $M1_T")
check "a member may not archive a class" "$ST" "403"
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/classes/$TINY" -H "authorization: Bearer $OWNER_T")
check "an owner may" "$ST" "200"
ST=$(code -X DELETE "$B/api/v1/gyms/$GYM/classes/$TINY" -H "authorization: Bearer $OWNER_T")
check "  twice is refused" "$ST" "409"

code "$B/api/v1/gyms/$GYM/classes?from=$WINDOW_FROM&to=$WINDOW_TO" -H "authorization: Bearer $M1_T" >/dev/null
check "  it leaves the timetable" \
  "$(pyq "any(x['class_id']=='$TINY' for x in d)")" "False"

# The booking that was held in it still exists — archiving is not deletion, and
# "who was in it" has to stay answerable.
ST=$(code "$B/api/v1/gyms/$GYM/classes/$TINY/roster?on_date=$MON" -H "authorization: Bearer $OWNER_T")
check "  but its roster is still readable" "$ST" "200"
check "  and still names who was in it" "$(pyq "len(d)")" "1"

ST=$(book "$M3_T" "$TINY" "$MON")
check "  and nobody new can book into it" "$ST" "400"

echo ""
echo "=== 9. tenant isolation ==="
OTHER_T=$(signup "cls-other-$S@example.com" "Ollie Other")
OTHER_GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OTHER_T" \
  -H 'content-type: application/json' -d "{\"name\":\"Rival Classes $S\"}" >/dev/null; jget "['id']" < $VTMP/b.json)
ST=$(code "$B/api/v1/gyms/$OTHER_GYM/classes?from=$WINDOW_FROM&to=$WINDOW_TO" -H "authorization: Bearer $M1_T")
check "a member of one gym cannot read another's timetable" "$ST" "404"
# The id exists, but not in a gym this caller belongs to.
ST=$(code "$B/api/v1/gyms/$OTHER_GYM/classes/$ZUMBA/roster?on_date=$MON" -H "authorization: Bearer $OTHER_T")
check "a class id from another gym is not found" "$ST" "404"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
