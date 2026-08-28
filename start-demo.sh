#!/usr/bin/env bash
# Linux: run this from a terminal, or double-click it if your file manager is
# set to run executable text files.
#
#   bash start-demo.sh
cd "$(dirname "$0")" || exit 1
exec bash demo/run.sh
