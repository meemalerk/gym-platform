#!/usr/bin/env bash
# macOS: double-click this file in Finder.
#
# If macOS refuses to open it ("cannot be opened because it is from an
# unidentified developer"), right-click it and choose Open instead — that
# offers an Open button the plain double-click does not.
cd "$(dirname "$0")" || exit 1
exec bash demo/run.sh
