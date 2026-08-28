#!/usr/bin/env bash
# Linux: stops the demo.
cd "$(dirname "$0")" || exit 1
exec bash demo/stop.sh
