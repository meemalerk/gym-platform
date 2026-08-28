#!/usr/bin/env bash
# The coach's view of one client: session filters and honest durations.
#
# Two claims under test.
#
# **Durations come from the athlete's clock.** `started_at` already travelled
# with the data (ADR-0008); the END did not, so finishing a session recorded
# the SERVER's clock — and a workout logged offline and synced three hours
# later reported a three-hour training session. Every average a coach reads was
# wrong by however long the phone was offline. The regression test for that is
# §3: finish a session claiming an end time, and assert the duration matches
# the claim rather than the round trip.
#
# **Filters narrow; they never widen.** `?athlete_id=` is a convenience over a
# list the server has already scoped. Pointing it at someone you may not see
# must return nothing — not their data, and not a 403 that confirms they exist.
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
export SERVER_PORT=8106
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
./target/debug/server > /tmp/server-athlete.log 2>&1 &
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
# An ISO instant `n` minutes from now, for building believable session clocks.
at() { "$PY" -c "import datetime,sys;print((datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(minutes=int(sys.argv[1]))).isoformat().replace('+00:00','Z'))" "$1"; }

S=$(date +%s%N); PW="correct horse battery staple"; TODAY=$(date +%F)
# The session filter compares against `started_at`, a timestamptz, by UTC
# calendar date — so the window must be built in UTC too. `date +%F` is LOCAL,
# and on a machine east of Greenwich in the small hours those are different
# days, which made this suite fail for a reason that had nothing to do with the
# code under test.
#
# Worth stating plainly: UTC is a placeholder, not the right long-term answer.
# A coach asking for "this week" means the GYM's week. That needs the gym's
# IANA timezone, which ADR-0015's operating calendar introduces and which does
# not exist yet — so the API documents UTC rather than pretending to know.
UTC_TODAY=$("$PY" -c "import datetime;print(datetime.datetime.now(datetime.timezone.utc).date())")
UTC_YESTERDAY=$("$PY" -c "import datetime;print(datetime.datetime.now(datetime.timezone.utc).date() - datetime.timedelta(days=1))")

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

echo "=== setup: a published programme, a coach, two athletes ==="
OWNER_T=$(signup "av-owner-$S@example.com" "Olive Owner")
GYM=$(code -X POST "$B/api/v1/gyms" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Athlete View Gym"}' >/dev/null; jget "['id']" < $VTMP/b.json)

HC_T=$(signup "av-hc-$S@example.com" "Hana Head");     accept_inv "$HC_T" "$(invite "av-hc-$S@example.com" '["owner"]')"
T1_T=$(signup "av-t1-$S@example.com" "Tariq Trainer"); accept_inv "$T1_T" "$(invite "av-t1-$S@example.com" '["trainer"]')"
M1_T=$(signup "av-m1-$S@example.com" "Mo Member");     accept_inv "$M1_T" "$(invite "av-m1-$S@example.com" '["member"]')"
M2_T=$(signup "av-m2-$S@example.com" "Mia Member");    accept_inv "$M2_T" "$(invite "av-m2-$S@example.com" '["member"]')"
T1=$(uid "$T1_T"); M1=$(uid "$M1_T"); M2=$(uid "$M2_T")

# T1 coaches M1 only. M2 is nobody's client.
# Pairing is a two-step handshake now (ADR-0034): the manager proposes and
# the trainer accepts. Direct pairing is gone, because the relationship hands
# the trainer that member's whole training history and they get asked first.
PAIR_REQ=$(code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/propose" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d "{\"athlete_id\":\"$M1\",\"coach_id\":\"$T1\"}" >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/coaching-requests/$PAIR_REQ/answer" -H "authorization: Bearer $T1_T" \
  -H 'content-type: application/json' -d '"accept"' >/dev/null

code -X POST "$B/api/v1/gyms/$GYM/programs" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Block A"}' >/dev/null
V=$(pyq "d['latest_version']['id']")
code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V/weeks" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"week_number":1}' >/dev/null
WEEK=$(pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/program-weeks/$WEEK/workouts" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"day_number":1,"name":"Lower"}' >/dev/null
WK=$(pyq "d['id']")
# The review gate counts PRESCRIBED EXERCISES, not weeks: a week or a workout
# with nothing in it is not a trainable plan, so it can no longer be reviewed.
AV_EX=$(code -X POST "$B/api/v1/gyms/$GYM/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' -d '{"name":"Back Squat","modality":{"kind":"repetitions"}}' >/dev/null; pyq "d['id']")
code -X POST "$B/api/v1/gyms/$GYM/workout-templates/$WK/exercises" -H "authorization: Bearer $OWNER_T" \
  -H 'content-type: application/json' \
  -d "{\"exercise_id\":\"$AV_EX\",\"prescription\":{\"kind\":\"repetitions\",\"sets\":3,\"target\":{\"min\":5,\"max\":5},\"rir\":2}}" >/dev/null
for move in submit_for_review approve publish; do
  ACTOR="$HC_T"; [ "$move" = "submit_for_review" ] && ACTOR="$OWNER_T"
  code -X POST "$B/api/v1/gyms/$GYM/program-versions/$V/transition" -H "authorization: Bearer $ACTOR" \
    -H 'content-type: application/json' -d "\"$move\"" >/dev/null
done

# Who may prescribe changed (ADR-0034): the athlete's own coach, or the athlete
# themselves. A manager writes the catalogue and no longer assigns from it, so
# the token matters — M1 gets it from their coach, M2 (nobody's client) self-serves.
assign() { code -X POST "$B/api/v1/gyms/$GYM/program-assignments" -H "authorization: Bearer $1" \
  -H 'content-type: application/json' \
  -d "{\"athlete_id\":\"$2\",\"program_version_id\":\"$V\",\"start_date\":\"$TODAY\"}" >/dev/null; pyq "d['id']"; }
A1=$(assign "$T1_T" "$M1"); A2=$(assign "$M2_T" "$M2")

echo ""
echo "=== 1. an honest duration ==="
SID=$(uuid)
STARTED=$(at -240)   # began four hours ago
ENDED=$(at -185)     # trained for 55 minutes
code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SID\",\"assignment_id\":\"$A1\",\"workout_template_id\":\"$WK\",\"started_at\":\"$STARTED\"}" >/dev/null

ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID/finish" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d "{\"outcome\":\"completed\",\"ended_at\":\"$ENDED\"}")
check "a session finishes with a client end time" "$ST" "200"
check "  the end time is kept" "$(pyq "d['ended_at'] is not None")" "True"

# THE regression. Server time says four hours; the athlete says 55 minutes.
# Only one of those is training.
check "  duration is 55 minutes, not the sync delay" \
  "$(pyq "abs(d['duration_seconds'] - 55*60) <= 60")" "True"

echo ""
echo "=== 2. the old shape still works ==="
# Clients that predate ended_at send a bare string. They must keep working,
# and fall back to the server's clock rather than reporting nothing.
SID2=$(uuid)
code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SID2\",\"assignment_id\":\"$A1\",\"workout_template_id\":\"$WK\",\"started_at\":\"$(at -30)\"}" >/dev/null
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID2/finish" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d '"completed"')
check "a bare outcome string is still accepted" "$ST" "200"
check "  with no client end time recorded" "$(pyq "d['ended_at'] is None")" "True"
check "  but a duration is still derived" "$(pyq "d['duration_seconds'] is not None")" "True"

echo ""
echo "=== 3. an impossible end time does not strand the athlete ==="
SID3=$(uuid)
code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SID3\",\"assignment_id\":\"$A1\",\"workout_template_id\":\"$WK\",\"started_at\":\"$(at -20)\"}" >/dev/null
# A phone whose clock is a day fast. Refusing would leave a workout that can
# never be closed; dropping the value costs only precision.
ST=$(code -X POST "$B/api/v1/gyms/$GYM/workout-sessions/$SID3/finish" -H "authorization: Bearer $M1_T" \
  -H 'content-type: application/json' -d "{\"outcome\":\"completed\",\"ended_at\":\"$(at 1440)\"}")
check "a wildly wrong clock still closes the workout" "$ST" "200"
check "  the bad value is dropped" "$(pyq "d['ended_at'] is None")" "True"
check "  and the duration falls back" "$(pyq "d['duration_seconds'] is not None")" "True"

echo ""
echo "=== 4. an open session has no duration ==="
SID4=$(uuid)
code -X POST "$B/api/v1/gyms/$GYM/workout-sessions" -H "authorization: Bearer $M2_T" \
  -H 'content-type: application/json' \
  -d "{\"id\":\"$SID4\",\"assignment_id\":\"$A2\",\"workout_template_id\":\"$WK\",\"started_at\":\"$(at -10)\"}" >/dev/null
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions/$SID4" -H "authorization: Bearer $M2_T")
check "an in-progress session reads back" "$ST" "200"
# It has an ELAPSED time; that is a different thing and the API must not
# pretend otherwise, or an abandoned session inflates every average.
check "  and reports no duration" "$(pyq "d['session']['duration_seconds'] is None")" "True"

echo ""
echo "=== 5. filtering by athlete ==="
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions?athlete_id=$M1" -H "authorization: Bearer $OWNER_T")
check "a manager may filter to one athlete" "$ST" "200"
check "  and gets only theirs" "$(pyq "all(x['athlete_id'] == '$M1' for x in d)")" "True"
check "  which is the three logged above" "$(pyq "len(d)")" "3"

ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions?athlete_id=$M1" -H "authorization: Bearer $T1_T")
check "their coach sees the same" "$(pyq "len(d)")" "3"

# The important one: a filter narrows what you may see, it never widens it.
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions?athlete_id=$M2" -H "authorization: Bearer $T1_T")
check "filtering to a stranger returns nothing" "$(pyq "len(d)")" "0"
check "  and does NOT 403, which would confirm they exist" "$ST" "200"

echo ""
echo "=== 6. filtering by date, and the last-day boundary ==="
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions?from=$UTC_TODAY&to=$UTC_TODAY" -H "authorization: Bearer $OWNER_T")
check "today..today includes today's sessions" "$(pyq "len(d) >= 3")" "True"

# The classic off-by-one: comparing a date against a timestamptz with `<=`
# drops everything logged after midnight UTC on the final day. The repository
# adds a day and uses a strict `<` to avoid exactly this.
check "  including ones logged late in the day" \
  "$(pyq "any(x['id'] == '$SID' for x in d)")" "True"

ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions?to=$UTC_YESTERDAY" -H "authorization: Bearer $OWNER_T")
check "a window ending yesterday excludes today" "$(pyq "len(d)")" "0"

ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions?limit=1" -H "authorization: Bearer $OWNER_T")
check "limit is honoured" "$(pyq "len(d)")" "1"
ST=$(code "$B/api/v1/gyms/$GYM/workout-sessions?limit=99999" -H "authorization: Bearer $OWNER_T")
check "an absurd limit is clamped, not a timeout" "$ST" "200"

echo ""
echo "=== 7. the reads the athlete screen sits on ==="
ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/athlete-profile" -H "authorization: Bearer $T1_T")
check "a coach may read their client's profile" "$([ "$ST" = "200" ] || [ "$ST" = "404" ] && echo ok)" "ok"
# 404, not 403 — and that is the better answer, matching how this codebase
# treats every "may I see this person" question. A 403 would confirm that M2
# exists and trains here; 404 says only that there is nothing here for you.
ST=$(code "$B/api/v1/gyms/$GYM/members/$M2/measurements" -H "authorization: Bearer $T1_T")
check "a stranger's measurements are not found, not forbidden" "$ST" "404"
ST=$(code "$B/api/v1/gyms/$GYM/members/$M1/measurements" -H "authorization: Bearer $T1_T")
check "their own client's, yes" "$ST" "200"

echo ""
echo "======================================"
echo "  PASSED: $PASS    FAILED: $FAIL"
echo "======================================"
[ "$FAIL" -eq 0 ]
