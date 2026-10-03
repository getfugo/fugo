#!/bin/sh
# Disk guard (docs/rust-port/REWRITE_PLAN.md §8.1): fails when the shared Rust target directory
# exceeds FUGO_TARGET_LIMIT_MB (default 8000 MB) or the root file system has less than
# 2 GB free. The plan's 2.5 GB budget (§8.1) assumed ~4.5 GB free; after the old port and its
# build output were deleted ~20 GB is free, so the budget was raised. Prints both numbers, as
# every task reports them when it ends.
#
#   tools/dev/disk.sh
#
# The target directory is $CARGO_TARGET_DIR, else target/ of the main checkout (shared by all
# worktrees).
set -eu

limit_target_kb=$((${FUGO_TARGET_LIMIT_MB:-8000} * 1024))
min_free_kb=$((2048 * 1024))

if [ -n "${CARGO_TARGET_DIR:-}" ]; then
	target=$CARGO_TARGET_DIR
else
	common=$(git -C "$(dirname "$0")" rev-parse --path-format=absolute --git-common-dir)
	target=$(dirname "$common")/target
fi

used_kb=0
if [ -d "$target" ]; then
	used_kb=$(du -sk "$target" | cut -f1)
fi
free_kb=$(df -Pk / | awk 'NR == 2 { print $4 }')

echo "target:    $((used_kb / 1024)) MB ($target), limit $((limit_target_kb / 1024)) MB"
echo "free on /: $((free_kb / 1024)) MB, minimum $((min_free_kb / 1024)) MB"

status=0
if [ "$used_kb" -gt "$limit_target_kb" ]; then
	echo "disk.sh: the target directory exceeds the budget" >&2
	status=1
fi
if [ "$free_kb" -lt "$min_free_kb" ]; then
	echo "disk.sh: less than $((min_free_kb / 1024)) MB free" >&2
	status=1
fi
exit $status
