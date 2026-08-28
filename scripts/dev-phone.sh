#!/usr/bin/env bash
# Start (or restart) everything needed to run the app on a physical phone.
#
#   phone -> 192.168.18.29:8080 -> WSL 8080 (lan-proxy) -> metro 8213 / api 8092
#
# Port 8080 is used because it is the one port already forwarded from Windows to
# WSL, so no elevated `netsh portproxy` rule is needed.
#
# NOTE: this machine also runs unrelated projects (8081/3001/3100, and as of
# 2026-07-25 a backend on **8090** — which is why the gym API lives on 8092).
# Nothing here touches them — process cleanup checks each pid's cwd first.
#
# Usage: bash scripts/dev-phone.sh [lan-ip]
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
LAN_IP="${1:-192.168.18.29}"
API_PORT=8092

echo "=== stopping only our own processes ==="
# pgrep -x (exact PROCESS NAME), never -f: -f matches the full command line, so
# any invoking shell whose -c string merely CONTAINS the word "node" matches and
# gets killed — which terminated the very pipeline that called this script. The
# name match cannot self-hit (a shell is named bash), and the cwd check still
# keeps hands off the unrelated project on this machine.
#
# "MainThread" is in the list because node ≥24 renames its main thread, so a
# running Metro no longer answers to `pgrep -x node` — a stale Metro that
# survives here keeps serving a bundle with the OLD inlined EXPO_PUBLIC_* env,
# which cost a real debugging session. The cwd guard stays the actual safety.
for pid in $(pgrep -x node; pgrep -x MainThread; pgrep -x server) ; do
  cwd=$(readlink -f "/proc/$pid/cwd" 2>/dev/null)
  case "$cwd" in
    "$PWD"*) kill "$pid" 2>/dev/null && echo "  stopped $pid";;
  esac
done
sleep 2

export DATABASE_URL="${DATABASE_URL:-postgres://gym:gym_dev_password@localhost:5455/gym}"
export APP_DATABASE_URL="${APP_DATABASE_URL:-postgres://gym_app:gym_app_dev_password@localhost:5455/gym}"
export JWT_SECRET="${JWT_SECRET:-dev-only-insecure-secret-change-me-before-any-real-deployment}"

echo ""
echo "=== API on $API_PORT ==="
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

cargo build --bin server 2>&1 | tail -2
setsid env SERVER_PORT="$API_PORT" SERVER_HOST=127.0.0.1 RUST_LOG=info \
  CORS_ALLOWED_ORIGINS="http://localhost:8080,http://127.0.0.1:8080,http://$LAN_IP:8080" \
  ./target/debug/server > /tmp/gym-api.log 2>&1 < /dev/null &
disown
for i in $(seq 1 40); do curl -fsS "http://127.0.0.1:$API_PORT/health" >/dev/null 2>&1 && break; sleep 0.5; done
echo "  $(curl -fsS "http://127.0.0.1:$API_PORT/health" || echo DOWN)"

echo ""
echo "=== Metro on 8213 ==="
(cd apps/mobile && setsid env \
  REACT_NATIVE_PACKAGER_HOSTNAME="$LAN_IP" \
  EXPO_PUBLIC_API_URL="http://$LAN_IP:8080" \
  EXPO_NO_TELEMETRY=1 \
  npx expo start --port 8213 --host lan \
  > /tmp/gym-metro.log 2>&1 < /dev/null & disown)
for i in $(seq 1 120); do curl -fsS http://127.0.0.1:8213/status >/dev/null 2>&1 && break; sleep 1; done
echo "  metro: $(curl -fsS -o /dev/null -w '%{http_code}' http://127.0.0.1:8213/status)"

echo ""
echo "=== LAN door on 8080 ==="
setsid node scripts/lan-proxy.mjs 8080 8213 "$API_PORT" > /tmp/gym-proxy.log 2>&1 < /dev/null &
disown
sleep 2

echo ""
echo "=== end-to-end checks ==="
echo "  health : $(curl -fsS --max-time 6 http://127.0.0.1:8080/health || echo FAIL)"
login=$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' -X POST \
  http://127.0.0.1:8080/api/v1/auth/login -H 'content-type: application/json' \
  -d '{"email":"owner@demo.test","password":"demopassword"}')
echo "  login  : HTTP $login $([ "$login" = "200" ] && echo '(a demo account can sign in)' || echo '(PROBLEM)')"
echo "  metro  : $(curl -fsS -o /dev/null -w '%{http_code}' --max-time 6 http://127.0.0.1:8080/status)"

echo ""
echo "  Open in Expo Go:  exp://$LAN_IP:8080"
