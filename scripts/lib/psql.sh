# Shared: how these suites reach Postgres. Source it, do not execute it.
#
#   . "$(dirname "$0")/lib/psql.sh"
#   "${PSQL_OWNER[@]}" <<'SQL' ... SQL
#
# Locally the database is the docker-compose container, addressed by the name
# compose gives it. On CI it is a SERVICE container with a generated name and
# no `gym-postgres` to exec into, so every statement came back
#
#   Error response from daemon: No such container: gym-postgres
#
# and the suites reported that as ordinary assertion failures - verify-rls.sh
# announced 0 passed, 7 failed, which reads as "the database has stopped
# isolating tenants" rather than "this script cannot find a database". The
# suite whose entire job is to prove RLS works was proving nothing, loudly.
#
# So: prefer a real psql client when the machine has one (CI runners ship one),
# and fall back to docker exec, which is the normal case on a development
# machine that has Docker but no Postgres client installed.
#
# Both roles connect over TCP either way. That matters for the app role: it has
# to authenticate by password through pg_hba rather than arrive over the local
# socket, because these suites exist to prove the application connects as a
# NON-OWNER. Owners bypass row-level security entirely.

GYM_DB_OWNER_URL="${DATABASE_URL:-postgres://gym:gym_dev_password@localhost:5455/gym}"
GYM_DB_APP_URL="${APP_DATABASE_URL:-postgres://gym_app:gym_app_dev_password@localhost:5455/gym}"

if command -v psql >/dev/null 2>&1; then
  PSQL_OWNER=(psql "$GYM_DB_OWNER_URL" -tAq)
  PSQL_APP=(psql "$GYM_DB_APP_URL" -tAq)
else
  # -i is required: these are fed SQL on stdin via heredocs.
  PSQL_OWNER=(docker exec -i gym-postgres psql -U gym -d gym -tAq)
  PSQL_APP=(docker exec -i -e PGPASSWORD=gym_app_dev_password gym-postgres
            psql -U gym_app -h 127.0.0.1 -d gym -tAq)
fi
