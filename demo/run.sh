#!/usr/bin/env bash
# Starts the demo and opens it in a browser. Written to be run by someone who
# has never opened a terminal on purpose, so every failure explains itself in
# plain words and says what to do next.
set -uo pipefail

WEB_PORT=8210
API_PORT=8211
COMPOSE_FILE="docker-compose.demo.yml"

# Run from the project folder no matter where this was launched from — a
# double-clicked script does not inherit a useful working directory.
cd "$(dirname "$0")/.." || exit 1

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
warn() { printf '\033[33m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1"; }

pause_if_double_clicked() {
  # Keep the window open so the message is readable rather than flashing past.
  if [ -t 0 ]; then
    echo ""
    read -r -p "Press Enter to close this window. " _ || true
  fi
}

echo ""
bold "Gym Platform — demo"
echo "This starts the app on your machine and opens it in your browser."
echo ""

# ------------------------------------------------------------ 1. Is Docker in?
if ! command -v docker >/dev/null 2>&1; then
  fail "Docker is not installed."
  echo ""
  echo "The demo needs Docker Desktop. It is free, and it is the only thing"
  echo "you need to install — everything else is handled for you."
  echo ""
  echo "  1. Download it from:  https://www.docker.com/products/docker-desktop/"
  echo "  2. Install it and start it (you may be asked to restart your computer)."
  echo "  3. Wait until the Docker icon stops animating."
  echo "  4. Run this file again."
  pause_if_double_clicked
  exit 1
fi

# ------------------------------------------------------- 2. Is Docker running?
if ! docker info >/dev/null 2>&1; then
  fail "Docker is installed but not running."
  echo ""
  echo "Start Docker Desktop, wait for its icon to stop animating (that can"
  echo "take a minute), then run this file again."
  pause_if_double_clicked
  exit 1
fi

# `docker compose` (v2, built in) vs the old standalone `docker-compose`.
if docker compose version >/dev/null 2>&1; then
  COMPOSE=(docker compose)
elif command -v docker-compose >/dev/null 2>&1; then
  COMPOSE=(docker-compose)
else
  fail "This version of Docker is too old — it has no 'compose' command."
  echo "Update Docker Desktop and try again."
  pause_if_double_clicked
  exit 1
fi

# --------------------------------------------------------- 3. Are ports free?
# Skipped when our own demo is already up — it is allowed to hold its own ports.
already_running=$("${COMPOSE[@]}" -f "$COMPOSE_FILE" ps -q 2>/dev/null | wc -l)

if [ "$already_running" -eq 0 ]; then
  port_in_use() {
    # No single portable tool for this, so try whichever exists and treat
    # "cannot tell" as free rather than blocking on a check we cannot run.
    if command -v lsof >/dev/null 2>&1; then
      lsof -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1 && return 0
    elif command -v ss >/dev/null 2>&1; then
      ss -ltn 2>/dev/null | grep -q ":$1 " && return 0
    fi
    return 1
  }

  for p in "$WEB_PORT" "$API_PORT"; do
    if port_in_use "$p"; then
      warn "Something else on this computer is already using port $p."
      echo "The demo may fail to start. If it does, close the other program,"
      echo "or change $p in $COMPOSE_FILE."
      echo ""
    fi
  done
fi

# ------------------------------------------------------------------ 4. Build
# Built as its own step, separately from starting, so a compiler error and a
# crashed container do not produce the same unhelpful message.
bold "Building…"
echo "The first run downloads and builds everything, which usually takes"
echo "5-15 minutes. After that it starts in seconds."
echo ""

if ! "${COMPOSE[@]}" -f "$COMPOSE_FILE" build; then
  echo ""
  fail "The build did not finish."
  echo ""
  echo "The real reason is in the messages above this one — scroll up."
  echo ""
  echo "The two usual causes:"
  echo "  • Docker has too little memory. Docker Desktop → Settings →"
  echo "    Resources → set memory to at least 4 GB → Apply & Restart."
  echo "  • No internet connection. The build downloads as it goes."
  pause_if_double_clicked
  exit 1
fi

# ------------------------------------------------------------------ 5. Start
echo ""
bold "Starting…"
echo ""

if ! "${COMPOSE[@]}" -f "$COMPOSE_FILE" up -d; then
  echo ""
  fail "The demo could not start."
  echo ""
  echo "To see what went wrong:"
  echo "  ${COMPOSE[*]} -f $COMPOSE_FILE logs"
  pause_if_double_clicked
  exit 1
fi

# ------------------------------------------------------- 6. Wait for the app
echo ""
printf 'Waiting for the app to come up'
URL="http://localhost:$WEB_PORT"
ready=0
for _ in $(seq 1 90); do
  if curl -fsS -o /dev/null --max-time 3 "$URL" 2>/dev/null; then ready=1; break; fi
  printf '.'
  sleep 2
done
echo ""

if [ "$ready" -ne 1 ]; then
  echo ""
  fail "The app did not answer in time."
  echo ""
  echo "It may still be finishing. Try opening $URL in your browser."
  echo "If that does not work, run:"
  echo "  ${COMPOSE[*]} -f $COMPOSE_FILE logs"
  pause_if_double_clicked
  exit 1
fi

# ---------------------------------------------------------- 7. Open a browser
case "$(uname -s)" in
  Darwin) open "$URL" >/dev/null 2>&1 || true ;;
  Linux)  xdg-open "$URL" >/dev/null 2>&1 || true ;;
esac

echo ""
bold "Ready — the app is at $URL"
echo ""
echo "If your browser did not open by itself, copy that address into it."
echo ""
bold "Sign in with any of these (the sign-in screen lists them as buttons):"
echo ""
echo "  owner@demo.test       runs the gym — members, billing, writes programmes"
echo "  trainer@demo.test     coaches a handful of members"
echo "  member@demo.test      trains here — the richest account to look at"
echo ""
echo "  The password for all of them is:  demopassword"
echo ""
echo "To stop the demo later, run stop-demo (next to this file)."
echo ""
pause_if_double_clicked
