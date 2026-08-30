#!/usr/bin/env bash
# Put the running demo on the internet for a while, and print a link to send.
#
# Uses Cloudflare's free "quick tunnel": no account, no card, no DNS. It hands
# out a random https://something.trycloudflare.com address that forwards to the
# demo on this machine.
#
# ONE ORIGIN IS THE WHOLE TRICK. nginx serves the app AND proxies /api to the
# backend, so a single tunnel covers both and the JS bundle calls the API
# relatively. A bundle with an absolute localhost URL would break the instant
# anyone else opened it — their browser would dial their own machine.
#
# Read the warning it prints. This is a real public URL.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

WEB_PORT=8210
COMPOSE_FILE="docker-compose.demo.yml"
BIN_DIR="demo/.bin"
LOG=/tmp/gym-tunnel.log

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
warn() { printf '\033[33m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1"; }

if docker compose version >/dev/null 2>&1; then
  COMPOSE=(docker compose)
else
  COMPOSE=(docker-compose)
fi

# ------------------------------------------------------- 1. is the demo up?
if ! curl -fsS -o /dev/null --max-time 3 "http://localhost:$WEB_PORT" 2>/dev/null; then
  echo "The demo is not running yet — starting it first."
  echo ""
  bash demo/run.sh || exit 1
fi

# --------------------------------------------------------- 2. get cloudflared
CF=""
if command -v cloudflared >/dev/null 2>&1; then
  CF=cloudflared
elif [ -x "$BIN_DIR/cloudflared" ]; then
  CF="$BIN_DIR/cloudflared"
# share.ps1 (what share-demo.bat runs) puts a .exe in this same folder.
# Git Bash runs it perfectly well, so there is no reason to fetch a second
# copy just because the name differs. Not -x: a downloaded .exe on Windows
# does not necessarily carry the executable bit.
elif [ -f "$BIN_DIR/cloudflared.exe" ]; then
  CF="$BIN_DIR/cloudflared.exe"
else
  echo "Fetching cloudflared (one-off, ~20 MB)…"
  mkdir -p "$BIN_DIR"
  # Default output name; the Windows arms override it with an .exe.
  OUT="$BIN_DIR/cloudflared"
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)   ASSET=cloudflared-linux-amd64 ;;
    Linux-aarch64)  ASSET=cloudflared-linux-arm64 ;;
    Darwin-arm64)   ASSET=cloudflared-darwin-arm64.tgz ;;
    Darwin-x86_64)  ASSET=cloudflared-darwin-amd64.tgz ;;
    # Git Bash reports MINGW64_NT-10.0-22631, which fell through to the
    # catch-all below: `bash demo/share.sh` - the command the README gives -
    # died on Windows with "No cloudflared build", while share-demo.bat
    # worked. Same script, same machine, different answer.
    MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64)
      ASSET=cloudflared-windows-amd64.exe; OUT="$BIN_DIR/cloudflared.exe" ;;
    MINGW*-i686|MSYS*-i686|CYGWIN*-i686)
      ASSET=cloudflared-windows-386.exe;   OUT="$BIN_DIR/cloudflared.exe" ;;
    *) fail "No cloudflared build for $(uname -s)-$(uname -m)."
       echo "Install it yourself: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
       exit 1 ;;
  esac

  URL="https://github.com/cloudflare/cloudflared/releases/latest/download/$ASSET"
  if [[ "$ASSET" == *.tgz ]]; then
    curl -fsSL "$URL" | tar -xz -C "$BIN_DIR" cloudflared || { fail "Download failed."; exit 1; }
  else
    curl -fsSL "$URL" -o "$OUT" || { fail "Download failed."; exit 1; }
  fi
  chmod +x "$OUT" 2>/dev/null || true
  CF="$OUT"
fi

# ------------------------------------------------------------ 3. open it up
echo ""
warn "About to put this demo on the public internet."
echo ""
echo "  • Anyone with the link can open it. The address is random and"
echo "    unguessable, but it is not password-protected."
echo "  • It carries the DEMO accounts, whose password is public knowledge."
echo "    Do not put anything real into it while it is shared."
echo "  • The link dies when you stop this — and a new one is different."
echo ""

: > "$LOG"
"$CF" tunnel --url "http://localhost:$WEB_PORT" --no-autoupdate > "$LOG" 2>&1 &
TUNNEL=$!
trap 'kill $TUNNEL 2>/dev/null; echo ""; echo "Tunnel closed. The link no longer works."' EXIT

printf 'Opening the tunnel'
PUBLIC=""
for _ in $(seq 1 40); do
  PUBLIC=$(grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' "$LOG" 2>/dev/null | head -1)
  [ -n "$PUBLIC" ] && break
  kill -0 $TUNNEL 2>/dev/null || break
  printf '.'
  sleep 1
done
echo ""

if [ -z "$PUBLIC" ]; then
  fail "Could not get a public address."
  echo ""
  tail -15 "$LOG"
  exit 1
fi

# Prove it end to end rather than trusting that the tunnel reported a URL:
# a link that 502s is worse than no link, because it gets sent anyway.
printf 'Checking it answers'
OK=0
for _ in $(seq 1 20); do
  if curl -fsS -o /dev/null --max-time 8 "$PUBLIC/health" 2>/dev/null; then OK=1; break; fi
  printf '.'
  sleep 2
done
echo ""

if [ "$OK" -ne 1 ]; then
  warn "The tunnel is up but the app did not answer through it yet."
  echo "Give it a few seconds and try $PUBLIC in a browser."
fi

echo ""
bold "Send them this link:"
echo ""
bold "    $PUBLIC"
echo ""
echo "It works on anything with a browser — Windows, Mac, an iPhone."
echo "On an iPhone, Safari → Share → Add to Home Screen makes it open"
echo "like a normal app, full screen."
echo ""
echo "Sign-in is a row of buttons; no typing needed. The password, if"
echo "they want to type one, is: demopassword"
echo ""

# A QR is the difference between "scan this" and "type 34 random characters"
# on a phone. Optional — no node, no QR, still a working link.
if command -v npx >/dev/null 2>&1; then
  echo "Or point their phone camera at this:"
  echo ""
  npx --yes qrcode-terminal "$PUBLIC" 2>/dev/null || echo "  (QR unavailable — just send the link)"
  echo ""
fi

bold "Leave this window open. Press Ctrl+C when you are done sharing."
wait $TUNNEL
