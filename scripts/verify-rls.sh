#!/usr/bin/env bash
# Proves row-level security actually isolates tenants at the DATABASE layer,
# independently of application code. See migrations/20260718000003.
#
# Usage: bash scripts/verify-rls.sh
set -uo pipefail

# How to reach Postgres. Locally it is the docker-compose container, addressed
# by the name docker-compose gives it. On CI it is a SERVICE container with a
# generated name and no `gym-postgres` to exec into, so every statement here
# came back "Error response from daemon: No such container" and the suite
# reported 0 passed, 7 failed - the one suite whose whole job is to prove the
# database enforces tenant isolation, silently proving nothing.
#
# So: prefer a real psql client when the machine has one (CI runners ship one),
# and fall back to docker exec, which is the normal case on a development
# machine that has Docker but no Postgres client installed.
#
# Both roles connect over TCP either way. That matters for the app role: it must
# authenticate by password through pg_hba rather than arriving over the local
# socket, because the point of this suite is that the app connects as a
# NON-OWNER (owners bypass RLS entirely).
OWNER_URL="${DATABASE_URL:-postgres://gym:gym_dev_password@localhost:5455/gym}"
APP_URL="${APP_DATABASE_URL:-postgres://gym_app:gym_app_dev_password@localhost:5455/gym}"

if command -v psql >/dev/null 2>&1; then
  PSQL_OWNER=(psql "$OWNER_URL" -tAq)
  PSQL_APP=(psql "$APP_URL" -tAq)
else
  # -i is required: these are fed SQL on stdin via heredocs.
  PSQL_OWNER=(docker exec -i gym-postgres psql -U gym -d gym -tAq)
  PSQL_APP=(docker exec -i -e PGPASSWORD=gym_app_dev_password gym-postgres
            psql -U gym_app -h 127.0.0.1 -d gym -tAq)
fi

pass=0; fail=0
check() { # check <label> <actual> <expected>
  if [ "$2" = "$3" ]; then echo "  PASS  $1 ($2)"; pass=$((pass+1));
  else echo "  FAIL  $1 — got '$2' want '$3'"; fail=$((fail+1)); fi
}

echo "=== seed: two gyms, one exercise each (as owner, bypassing RLS) ==="
"${PSQL_OWNER[@]}" <<'SQL' >/dev/null
DELETE FROM exercises WHERE name LIKE 'RLS Probe%';
DELETE FROM gyms WHERE slug IN ('rls-alpha','rls-beta');
INSERT INTO gyms (id, name, slug) VALUES
  ('11111111-1111-7111-8111-111111111111','RLS Alpha','rls-alpha'),
  ('22222222-2222-7222-8222-222222222222','RLS Beta','rls-beta');
INSERT INTO exercises (id, gym_id, name, modality) VALUES
  ('aaaaaaaa-1111-7111-8111-111111111111','11111111-1111-7111-8111-111111111111','RLS Probe Alpha','repetitions'),
  ('bbbbbbbb-2222-7222-8222-222222222222','22222222-2222-7222-8222-222222222222','RLS Probe Beta','repetitions');
SQL
echo "  seeded"

echo ""
echo "=== 1. app role WITHOUT tenant context must see nothing (fail closed) ==="
n=$("${PSQL_APP[@]}" -c "SELECT count(*) FROM exercises WHERE name LIKE 'RLS Probe%';")
check "no context -> 0 rows" "$n" "0"

echo ""
echo "=== 2. app role scoped to Alpha sees only Alpha ==="
out=$("${PSQL_APP[@]}" <<'SQL'
BEGIN;
SELECT set_config('app.current_gym','11111111-1111-7111-8111-111111111111',true);
SELECT string_agg(name, ',' ORDER BY name) FROM exercises WHERE name LIKE 'RLS Probe%';
COMMIT;
SQL
)
check "Alpha context" "$(echo "$out" | grep 'RLS Probe' || true)" "RLS Probe Alpha"

echo ""
echo "=== 3. app role scoped to Beta sees only Beta ==="
out=$("${PSQL_APP[@]}" <<'SQL'
BEGIN;
SELECT set_config('app.current_gym','22222222-2222-7222-8222-222222222222',true);
SELECT string_agg(name, ',' ORDER BY name) FROM exercises WHERE name LIKE 'RLS Probe%';
COMMIT;
SQL
)
check "Beta context" "$(echo "$out" | grep 'RLS Probe' || true)" "RLS Probe Beta"

echo ""
echo "=== 4. context is TRANSACTION-scoped, not session-scoped ==="
# The pooling bug this guards against: if SET leaked past COMMIT, the next
# statement on the same connection would still see Alpha's rows.
# Note: after COMMIT the GUC reverts to '' (empty string), NOT unset — which is
# why the policies use nullif(). Without that this query raises
# "invalid input syntax for type uuid" instead of returning 0.
out=$("${PSQL_APP[@]}" <<'SQL' 2>&1
BEGIN;
SELECT set_config('app.current_gym','11111111-1111-7111-8111-111111111111',true);
COMMIT;
SELECT 'AFTER=' || count(*) FROM exercises WHERE name LIKE 'RLS Probe%';
SQL
)
check "context gone after COMMIT" "$(echo "$out" | grep -o 'AFTER=[0-9]*' || echo "ERRORED")" "AFTER=0"

echo ""
echo "=== 5. cannot INSERT into another gym (WITH CHECK) ==="
err=$("${PSQL_APP[@]}" <<'SQL' 2>&1
BEGIN;
SELECT set_config('app.current_gym','11111111-1111-7111-8111-111111111111',true);
INSERT INTO exercises (id, gym_id, name, modality)
VALUES ('cccccccc-3333-7333-8333-333333333333','22222222-2222-7222-8222-222222222222','RLS Probe Smuggled','repetitions');
COMMIT;
SQL
)
if echo "$err" | grep -qi "row-level security"; then
  check "cross-tenant INSERT rejected" "rejected" "rejected"
else
  check "cross-tenant INSERT rejected" "ALLOWED: $err" "rejected"
fi

echo ""
echo "=== 6. cannot UPDATE another gym's row ==="
out=$("${PSQL_APP[@]}" <<'SQL'
BEGIN;
SELECT set_config('app.current_gym','11111111-1111-7111-8111-111111111111',true);
UPDATE exercises SET name = 'Renamed by Alpha' WHERE name = 'RLS Probe Beta';
COMMIT;
SQL
)
still=$("${PSQL_OWNER[@]}" -c "SELECT count(*) FROM exercises WHERE name = 'RLS Probe Beta';")
check "Beta's row untouched" "$still" "1"

echo ""
echo "=== 7. the owner role BYPASSES RLS (why the app must not use it) ==="
n=$("${PSQL_OWNER[@]}" -c "SELECT count(*) FROM exercises WHERE name LIKE 'RLS Probe%';")
check "owner sees both gyms" "$n" "2"

echo ""
echo "=== cleanup ==="
"${PSQL_OWNER[@]}" <<'SQL' >/dev/null
DELETE FROM exercises WHERE name LIKE 'RLS Probe%' OR name = 'Renamed by Alpha';
DELETE FROM gyms WHERE slug IN ('rls-alpha','rls-beta');
SQL
echo "  done"

echo ""
echo "======================================"
echo "  PASSED: $pass    FAILED: $fail"
echo "======================================"
exit "$fail"
