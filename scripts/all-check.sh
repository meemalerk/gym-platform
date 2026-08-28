#!/usr/bin/env bash
cd "$(dirname "$0")/.." || exit 1
export CARGO_TERM_COLOR=never
export DATABASE_URL="postgres://gym:gym_dev_password@localhost:5455/gym"

# The pure-logic suites import the app's .ts modules directly. Node 23.6+ strips
# types with no flag; 22.x needs to be asked. Detect rather than demand — CI and
# a dev laptop are rarely on the same minor, and the failure mode without this
# (ERR_UNKNOWN_FILE_EXTENSION) reads as a broken script, not a stale runtime.
NODE_MAJOR=$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)
NODE_MINOR=$(node -p 'process.versions.node.split(".")[1]' 2>/dev/null || echo 0)
if [ "$NODE_MAJOR" -lt 23 ] || { [ "$NODE_MAJOR" -eq 23 ] && [ "$NODE_MINOR" -lt 6 ]; }; then
  export NODE_OPTIONS="${NODE_OPTIONS:-} --experimental-strip-types --no-warnings"
fi

# NOTE: deliberately does NOT kill running servers. Each verify script starts
# its own on a dedicated port (8097/8098/8099), so killing here only breaks the
# long-running dev server the phone is connected to — which is exactly what
# happened once.

cargo fmt --all
echo "=== clippy ==="
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
echo "=== sqlx cache ==="
cargo sqlx prepare --workspace -- --all-targets 2>&1 | tail -1
echo "=== unit tests ==="
cargo test --workspace 2>&1 | grep -E "^test result" | \
  awk -F'[ ;]' '{p+=$4; f+=$6} END {print "  passed: " p "  failed: " f}'
echo "=== e2e (RLS enforced) ==="
# grep, not tail: on failure e2e.sh dumps the server log AFTER its summary, and
# tail -3 then shows only log lines — a failing suite reported as silence.
bash scripts/e2e.sh 2>&1 | grep -E "  FAIL |PASSED:" | tail -6
echo "=== standing: how staff are made (ADR-0031) ==="
bash scripts/verify-capacities.sh 2>&1 | tail -3
echo "=== rls isolation ==="
bash scripts/verify-rls.sh 2>&1 | tail -3
echo "=== programme immutability ==="
bash scripts/verify-program-immutability.sh 2>&1 | tail -2
echo "=== programme authoring (http) ==="
bash scripts/verify-programs.sh 2>&1 | tail -3
echo "=== coaching relationships (http) ==="
bash scripts/verify-coaching.sh 2>&1 | tail -3
echo "=== programme assignments (http) ==="
bash scripts/verify-assignments.sh 2>&1 | tail -3
echo "=== profiles (http) ==="
bash scripts/verify-profiles.sh 2>&1 | tail -3
echo "=== recommendations (http) ==="
bash scripts/verify-recommendations.sh 2>&1 | tail -3
echo "=== goals (http) ==="
bash scripts/verify-goals.sh 2>&1 | tail -3
echo "=== workout execution (http) ==="
bash scripts/verify-execution.sh 2>&1 | tail -3
echo "=== unplanned sessions (http) ==="
bash scripts/verify-unplanned-sessions.sh 2>&1 | tail -3
echo "=== billing (http) ==="
bash scripts/verify-billing.sh 2>&1 | tail -3
echo "=== entitlements (http) ==="
bash scripts/verify-entitlements.sh 2>&1 | tail -3
echo "=== gym check-ins (http) ==="
bash scripts/verify-checkins.sh 2>&1 | tail -3
echo "=== trainer authority (http) ==="
bash scripts/verify-trainer-authority.sh 2>&1 | tail -3
echo "=== coaching requests (http) ==="
bash scripts/verify-coaching-requests.sh 2>&1 | tail -3
echo "=== open registration (http) ==="
bash scripts/verify-open-registration.sh 2>&1 | tail -3
echo "=== athlete view & session filters (http) ==="
bash scripts/verify-athlete-view.sh 2>&1 | tail -3
echo "=== worker: recurring billing & outbox ==="
bash scripts/verify-worker.sh 2>&1 | tail -3
echo "=== card payment end to end ==="
bash scripts/verify-payments.sh 2>&1 | tail -3
echo "=== auth hardening (reset, verification, throttle) ==="
bash scripts/verify-auth-hardening.sh 2>&1 | tail -3
echo "=== operating calendar (http) ==="
bash scripts/verify-calendar.sh 2>&1 | tail -3
echo "=== group classes and bookings (http) ==="
bash scripts/verify-classes.sh 2>&1 | tail -3
echo "=== navigation manifest ==="
node scripts/verify-nav.mjs 2>&1 | tail -2
echo "=== landing redirect vs mount guards ==="
node scripts/verify-routing.mjs 2>&1 | tail -2
echo "=== activity formatting ==="
node scripts/verify-activity.mjs 2>&1 | tail -2
echo "=== progress metrics ==="
node scripts/verify-progress.mjs 2>&1 | tail -2
echo "=== class timetable shaping ==="
node scripts/verify-timetable.mjs 2>&1 | tail -2
echo "=== prescription rendering ==="
node scripts/verify-program-format.mjs 2>&1 | tail -2
echo "=== session naming ==="
node scripts/verify-session-name.mjs 2>&1 | tail -2
echo "=== attendance & durations ==="
node scripts/verify-attendance.mjs 2>&1 | tail -2
echo "=== today: next workout & attention ==="
node scripts/verify-today.mjs 2>&1 | tail -2
echo "=== plate maths ==="
node scripts/verify-plates.mjs 2>&1 | tail -2
echo "=== entitlement wording ==="
node scripts/verify-entitlement-words.mjs 2>&1 | tail -2
echo "=== palette contrast (WCAG AA) ==="
node scripts/verify-contrast.mjs 2>&1 | tail -2
echo "=== design consistency ==="
node scripts/verify-design-consistency.mjs 2>&1 | tail -2
echo "=== documentation links ==="
node scripts/verify-docs.mjs 2>&1 | tail -2
echo "=== design tokens are generated, not copied ==="
# Regenerate and diff. A hand-edited tokens.css would be a SECOND palette —
# unverified by verify-contrast.mjs and free to drift from the app's. This
# check is what makes "one source of truth" a fact rather than an intention.
node scripts/generate-console-tokens.mjs >/dev/null 2>&1
if git diff --quiet -- apps/console/src/tokens.css 2>/dev/null; then
  echo "  tokens in sync with apps/mobile/src/ui/theme.ts"
else
  echo "  FAIL  apps/console/src/tokens.css is stale or hand-edited"
  echo "        run: node scripts/generate-console-tokens.mjs"
fi
echo "=== mobile ==="
(cd apps/mobile && npx tsc --noEmit && echo "  tsc clean")
echo "=== console ==="
(cd apps/console && npx tsc --noEmit && echo "  tsc clean")
