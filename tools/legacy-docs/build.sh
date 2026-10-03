#!/usr/bin/env bash
# Builds the legacy docs site (testdata/legacy-docs, the test fixture that was docs/ until fugo
# got its own documentation) with the binary, as https://getfugo.github.io/ published it:
# the Tera overlay sites/docs (`sites.py make docs-live`, no patches) and the site's own node
# modules (`npm ci` of testdata/legacy-docs/package.json with tools/legacy-docs/package-lock.json,
# which resolves to the versions the published site was built with), the way the Go release
# workflow built it (`npm install`, then the Go version's build: no --minify, the production
# environment, network access for GetRemote: GitHub stars and releases, X posts, a font).
# Gate A-D3 (crates/cli/tests/it/docs.rs) compares this site with the published one. fugo's
# own documentation, docs/, is built by tools/docs/build.sh.
#
#   tools/legacy-docs/build.sh [-o <dir>] [--serve] [-- <binary args>...]
#
#   -o <dir>   the publish directory (default: <work>/public); emptied first
#   --serve    run the `server` command on the generated site instead of building it (live reload;
#              edits go to the generated site in <work>, not to testdata/legacy-docs or sites/docs)
#   -- ...     further arguments for the binary (e.g. --minify, -b <baseURL>, -p 1314)
#
# Environment:
#   FUGO_BINARY          the binary (default: target/release/<name>, built with
#                           `cargo build --release --locked -p ssg-cli` when missing)
#   FUGO_DOCS_WORK       work directory, outside the repository (default:
#                           ${TMPDIR:-/tmp}/ssg-legacy-docs): the generated site (rewritten on
#                           every run), the node modules (kept per lock file hash), public/
#   FUGO_GH_TOKEN        optional GitHub token for the GitHub API requests
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

WORK=${FUGO_DOCS_WORK:-${TMPDIR:-/tmp}/ssg-legacy-docs}
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
command -v npm >/dev/null || { log "npm is needed (the docs site's Alpine.js and Turbo)"; exit 1; }

# The site: testdata/legacy-docs unpatched with the Tera overlay.
site=$WORK/docs-live
rm -rf "$site"
python3 "$ROOT/tools/rust-port/i01/sites.py" make docs-live "$site" --overlay "$ROOT/sites/docs" >/dev/null

# The docs site's node modules, installed once per lock file.
lock=$HERE/package-lock.json
lock_hash=$(python3 -c 'import hashlib, sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest()[:16])' "$lock")
npm_dir=$WORK/npm-$lock_hash
if [ ! -d "$npm_dir/node_modules" ]; then
	log "installing the docs node modules (npm ci, tools/legacy-docs/package-lock.json)"
	rm -rf "$npm_dir.tmp"
	mkdir -p "$npm_dir.tmp"
	cp "$ROOT/testdata/legacy-docs/package.json" "$lock" "$npm_dir.tmp/"
	(cd "$npm_dir.tmp" && npm ci --no-audit --no-fund --loglevel=error >&2)
	rm -rf "$npm_dir"
	mv "$npm_dir.tmp" "$npm_dir"
fi
ln -s "$npm_dir/node_modules" "$site/node_modules"

cd "$site"
if [ -n "$serve" ]; then
	exec "$BIN" server "${extra[@]}"
fi
rm -rf "$out"
"$BIN" -d "$out" "${extra[@]}"
log "published to $out"
