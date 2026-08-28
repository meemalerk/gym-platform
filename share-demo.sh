#!/usr/bin/env bash
# Get a public link to send to someone. Mac/Linux.
cd "$(dirname "$0")" || exit 1
exec bash demo/share.sh
