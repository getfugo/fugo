#!/usr/bin/env bash
# Builds fugo's documentation site, https://getfugo.github.io/, from docs/ with the binary. The
# site needs no node tools and no network: Sass and the script are compiled in process. The
# processed images and the caches go to the work directory, so nothing is written into docs/.
# The test `docs_site` (crates/cli/tests/it/docs_site.rs) builds it the same way and fails on
# warnings.
#
#   tools/docs/build.sh [-o <dir>] [--serve] [-- <binary args>...]
#
#   -o <dir>   the publish directory (default: <work>/public); emptied first
#   --serve    run the `server` command instead (live reload of edits in docs/)
#   -- ...     further arguments for the binary (e.g. --minify, -b <baseURL>, -p 1314)
#
# Environment:
#   FUGO_BINARY          the binary (default: target/release/<name>, built with
#                           `cargo build --release --locked -p ssg-cli` when missing)
#   FUGO_DOCS_WORK       work directory, outside the repository (default:
#                           ${TMPDIR:-/tmp}/ssg-docs): public/, resources/ (processed images)
#                           and cache/
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
log() { echo "build.sh: $*" >&2; }
usage() {
	sed -n '2,/^set -euo/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//' >&2
	exit 2
}

out= serve=
extra=()
while [ $# -gt 0 ]; do
	case $1 in
	-o) out=$2; shift 2 ;;
	--serve) serve=1; shift ;;
	--) shift; extra=("$@"); break ;;
	-h | --help) usage ;;
	*) log "unknown argument $1"; usage ;;
	esac
done

WORK=${FUGO_DOCS_WORK:-${TMPDIR:-/tmp}/ssg-docs}
case "$WORK" in "$ROOT" | "$ROOT"/*) log "the work directory must be outside the repository"; exit 2 ;; esac
mkdir -p "$WORK"
WORK=$(cd "$WORK" && pwd)
out=${out:-$WORK/public}

BIN=${FUGO_BINARY:-}
if [ -z "$BIN" ]; then
	target=${CARGO_TARGET_DIR:-$ROOT/target}
	case $target in /*) ;; *) target=$ROOT/$target ;; esac
	# The binary's name: `[[bin]] name` of crates/cli/Cargo.toml.
	name=$(awk -F'"' '/^\[\[bin\]\]/ { b = 1 } b && /^name/ { print $2; exit }' "$ROOT/crates/cli/Cargo.toml")
	BIN=$target/release/$name
	if [ ! -x "$BIN" ]; then
		log "building $name (cargo build --release --locked -p ssg-cli)"
		(cd "$ROOT" && cargo build --release --locked -p ssg-cli >&2)
	fi
fi

# `resourceDir` through its environment override: processed images outside docs/.
export FUGO_RESOURCEDIR=$WORK/resources
args=(-s "$ROOT/docs" --cache-dir "$WORK/cache")
if [ -n "$serve" ]; then
	exec "$BIN" server "${args[@]}" ${extra[@]+"${extra[@]}"}
fi
rm -rf "$out"
"$BIN" build "${args[@]}" -d "$out" ${extra[@]+"${extra[@]}"}
log "published to $out"
