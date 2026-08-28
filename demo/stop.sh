#!/usr/bin/env bash
# Stops the demo. Leaves the data alone, so starting it again is instant and
# everything is where you left it.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

if docker compose version >/dev/null 2>&1; then
  COMPOSE=(docker compose)
else
  COMPOSE=(docker-compose)
fi

echo ""
echo "Stopping the demo…"
"${COMPOSE[@]}" -f docker-compose.demo.yml down
echo ""
echo "Stopped. Run start-demo again whenever you want it back —"
echo "your data is still there and it will start in seconds."
echo ""
echo "To erase the demo data as well:"
echo "  ${COMPOSE[*]} -f docker-compose.demo.yml down -v"
echo ""

if [ -t 0 ]; then
  read -r -p "Press Enter to close this window. " _ || true
fi
