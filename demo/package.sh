#!/usr/bin/env bash
# Build the zip you hand to someone who has never opened a terminal.
#
#   bash demo/package.sh [output.zip]
#
# The layout is the point. A raw checkout opens onto thirty items — Cargo.toml,
# .sqlx, crates, bins — and a non-technical person has no way to guess which one
# to click. Windows Explorer also lists every folder BEFORE every file, so no
# amount of clever naming floats the instructions to the top.
#
# So the zip is rearranged: seven plainly-named things at the root, and the
# entire repository tucked into "source code/", which they can ignore forever.
#
#   Gym Platform/
#     START HERE.html                  ← opens in a browser, double-clickable
#     Start the app (Windows).bat
#     Start the app (Mac).command
#     Stop the app (Windows).bat
#     Stop the app (Mac).command
#     Share a link (Windows).bat
#     Share a link (Mac).command
#     source code/                     ← everything else
#
# The contents come from `git archive`, so only TRACKED files ship: no target/,
# no node_modules/, and — the one that matters — no .env.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

OUT="${1:-gym-platform-demo.zip}"
STAGE=$(mktemp -d)
TOP="Gym Platform"
trap 'rm -rf "$STAGE"' EXIT

if [ -n "$(git status --porcelain)" ]; then
  echo "Note: you have uncommitted changes. git archive ships the last COMMIT,"
  echo "so anything unstaged will not be in the zip."
  echo ""
  git status --short
  echo ""
  read -r -p "Carry on anyway? [y/N] " reply
  case "$reply" in
    [yY]*) ;;
    *) echo "Stopped."; exit 1 ;;
  esac
fi

mkdir -p "$STAGE/$TOP/source code"
git archive --format=tar HEAD | tar -x -C "$STAGE/$TOP/source code" || exit 1

# The instructions belong at the top, not three folders down.
mv "$STAGE/$TOP/source code/START HERE.html" "$STAGE/$TOP/START HERE.html"

# Friendly launchers. Thin wrappers rather than moved files: the originals stay
# where the repository expects them, and these say what they do in words a
# person reads rather than a filename a developer greps for.
win() { # win <file> <script>
  # CRLF, because a .bat with Unix line endings misbehaves on some Windows
  # versions in ways that look like the file being broken.
  printf '@echo off\r\ncd /d "%%~dp0source code"\r\npowershell -NoProfile -ExecutionPolicy Bypass -File "demo\\%s"\r\n' \
    "$2" > "$STAGE/$TOP/$1"
}
mac() { # mac <file> <script>
  printf '#!/usr/bin/env bash\ncd "$(dirname "$0")/source code" || exit 1\nexec bash "demo/%s"\n' \
    "$2" > "$STAGE/$TOP/$1"
  chmod +x "$STAGE/$TOP/$1"
}

win "Start the app (Windows).bat" "run.ps1"
win "Stop the app (Windows).bat"  "stop.ps1"
win "Share a link (Windows).bat"  "share.ps1"
mac "Start the app (Mac).command" "run.sh"
mac "Stop the app (Mac).command"  "stop.sh"
mac "Share a link (Mac).command"  "share.sh"

# python3, not the `zip` command: `zip` is absent on a plain Ubuntu and needs
# root to add, while python3 is already required by the seed. demo/make-zip.py
# also carries the executable bit into the archive, which is what lets a Mac
# double-click a .command file at all.
# Absolute-or-relative: the caller may pass either, and prefixing $PWD onto an
# already-absolute path produces a directory that does not exist.
case "$OUT" in
  /*) OUT_ABS="$OUT" ;;
  *)  OUT_ABS="$PWD/$OUT" ;;
esac
python3 demo/make-zip.py "$STAGE" "$TOP" "$OUT_ABS" >/dev/null || {
  echo "Could not create the zip."
  exit 1
}

SIZE=$(du -h "$OUT_ABS" | cut -f1)
echo ""
echo "Wrote $OUT ($SIZE)"
echo ""
echo "What they see when they unzip it:"
python3 demo/list-zip.py "$OUT_ABS" top
echo ""

# Belt and braces. `git archive` cannot include an untracked file, but a secret
# leaving the building is worth two lines of paranoia.
LEAKED=$(python3 demo/list-zip.py "$OUT_ABS" leaks)
if [ -n "$LEAKED" ]; then
  echo "STOP — the zip contains things it should not:"
  echo "$LEAKED" | sed 's/^/    /'
  exit 1
fi
echo "Checked: no .env, no target/, no node_modules/ in the zip."
echo ""
echo "Send that file. Tell them to unzip it and open 'START HERE.html'."
