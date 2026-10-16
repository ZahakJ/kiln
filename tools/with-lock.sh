#!/usr/bin/env bash
# Kiln — run a command while holding the shared agent lock (PLAN.md §4.7).
# Every git write in the shared working tree goes through this wrapper so two agents never
# commit, rebase or push at the same time. Waits up to 10 minutes for the lock.
#   tools/with-lock.sh git pull --rebase origin main
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# util-linux flock takes the command directly after the lock file; a `--` there would be run as the
# command (PLAN.md §4.7 writes `flock -w 600 .agent.lock -- "$@"`, which fails with util-linux 2.39).
exec flock -w 600 "$repo/.agent.lock" "$@"
