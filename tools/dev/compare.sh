#!/usr/bin/env bash
# Acceptance comparison of one site (docs/rust-port/REWRITE_PLAN.md §7.2, §7.3): builds the
# candidate (the Rust build), compares it with the reference (the Go build's committed golden
# data) through tools/dev/structdiff.py, level by level (L1-L4, the structure oracle, A7),
# and applies the ratchet (testdata/baselines/<label>.json).
#
#   tools/dev/compare.sh <site> [--docs-patches i01|reduced|live] [--ref golden]
#                            [--task ID]... [--update] [--report-only] [--show N]
#
# Sites: testsite, docs-i01, docs-reduced, docs-live (`docs --docs-patches <variant>`
# is the same as `docs-<variant>`).
#
# Sides:
#   --ref golden   the reference, the only one: the committed golden data of
#                  testdata/golden/<label> (manifests of both passes and the structure dump;
#                  T01's oracle.sh wrote them with the Go build, frozen at 44529028)
#   candidate      this port: the site from `sites.py make <label> --overlay sites/<site>`;
#                  its structure dump comes from the unminified build (FUGO_STRUCTURE_OUT)
#
# The candidate builds each pass from a freshly generated site, from the site directory, with
#   <binary> --clock 2026-09-27T12:00:00Z --cacheDir <cache> [--minify] -d <out>
# (the minified pass gives L1 and L4, the unminified pass L1, L2 and L3) in the clean
# environment of the golden builds: HOME and the cache directory in the work directory (the
# site's golden GetRemote entries, `sites.py cache`), TZ=UTC, every proxy
# variable pointing at a refusing port (outbound HTTP disabled), and the node modules of
# tools/dev/node.sh as a `node_modules` symlink in the site (`js_build` imports from it; the
# binary installs nothing into a link). Manifests come from tools/dev/manifest.py
# (the candidate's with --full-text, for the A7 similarity of the worst pages).
#
# docs-live (gate A-D3) is the docs site as getfugo.github.io publishes it, and its golden data is
# the published site (testdata/golden/README.md): one unminified pass (the site is published
# unminified) giving L1-L4, no structure dump, the clock of the published build
# (2025-10-13T15:00:00Z), and the GetRemote responses of that day from `sites.py cache`.
#
# Ratchet: --task names the changes files (tools/dev/changes/<ID>.md) that list this task's
# changes; an unlisted new difference fails; --update writes the baseline for the listed changes;
# --report-only never fails. Without a baseline file every difference is new.
#
# Environment:
#   KEEP=1                  keep the sites, outputs, manifests and logs of the work directory
#   FUGO_COMPARE_WORK    work directory (default: $TMPDIR/ssg-compare), outside the repo
#   FUGO_BINARY          the binary (default: `cargo build --offline --locked -p
#                           ssg-cli`, then a copy of target/debug/<name> in the work dir)
#   FUGO_TASK            default for --task
# The report and structdiff.json stay in the work directory (<work>/<label>/); everything else
# there is deleted unless KEEP=1.
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
SITES_PY=$ROOT/tools/rust-port/i01/sites.py
MANIFEST_PY=$HERE/manifest.py
STRUCTDIFF_PY=$HERE/structdiff.py
CLOCK=2026-09-27T12:00:00Z

log() { echo "compare.sh: $*" >&2; }
usage() {
	sed -n '2,/^set -euo/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//' >&2
	exit 2
}

site= variant= ref=golden update= report_only= show=40
tasks=()
[ -n "${FUGO_TASK:-}" ] && tasks+=("$FUGO_TASK")
while [ $# -gt 0 ]; do
	case $1 in
	--docs-patches) variant=$2; shift 2 ;;
	--ref) ref=$2; shift 2 ;;
	--task) tasks+=("$2"); shift 2 ;;
	--update) update=1; shift ;;
	--report-only) report_only=1; shift ;;
	--show) show=$2; shift 2 ;;
	-h | --help) usage ;;
	-*) log "unknown option $1"; usage ;;
	*) [ -z "$site" ] || usage; site=$1; shift ;;
	esac
done
[ -n "$site" ] || usage

case $site in
docs) label=docs-${variant:-i01} ;;
docs-i01 | docs-reduced | docs-live)
	[ -z "$variant" ] || [ "docs-$variant" = "$site" ] || { log "$site contradicts --docs-patches $variant"; exit 2; }
	label=$site ;;
testsite)
	[ -z "$variant" ] || { log "--docs-patches applies to docs only"; exit 2; }
	label=$site ;;
*) log "unknown site $site (testsite, docs-i01, docs-reduced, docs-live)"; exit 2 ;;
esac
# docs-live: the published site's one unminified pass (L1-L4), its clock, no structure dump.
passes="minified unminified" unmin_levels=L1,L2,L3 structure_dump=1
if [ "$label" = docs-live ]; then
	passes=unminified unmin_levels=L1,L2,L3,L4 structure_dump=
	CLOCK=2025-10-13T15:00:00Z
fi
overlay=$ROOT/sites/${label%%-*}

NODE_MODULES=$("$HERE/node.sh" path)
GOLDEN=$ROOT/testdata/golden/$label
BASELINE=$ROOT/testdata/baselines/$label.json
WORK_ROOT=${FUGO_COMPARE_WORK:-${TMPDIR:-/tmp}/ssg-compare}
W=$WORK_ROOT/$label
case "$WORK_ROOT" in "$ROOT" | "$ROOT"/*) log "the work directory must be outside the repository"; exit 2 ;; esac

[ "$ref" = golden ] || { log "--ref golden (the only reference)"; exit 2; }
[ -d "$GOLDEN" ] || { log "no golden data at $GOLDEN"; exit 2; }
[ -d "$NODE_MODULES" ] || log "warning: no node modules at $NODE_MODULES (tools/dev/node.sh)"

rm -rf "$W"
mkdir -p "$W"
cleanup() {
	[ "${KEEP:-}" = 1 ] && return
	find "$W" -mindepth 1 -maxdepth 1 ! -name report.txt ! -name structdiff.json -exec rm -rf {} +
}
trap cleanup EXIT

program_binary() {
	if [ -n "${FUGO_BINARY:-}" ]; then
		echo "$FUGO_BINARY"
		return
	fi
	# The binary's name: `[[bin]] name` of crates/cli/Cargo.toml.
	local name
	name=$(awk -F'"' '/^\[\[bin\]\]/ { b = 1 } b && /^name/ { print $2; exit }' "$ROOT/crates/cli/Cargo.toml")
	log "building $name (cargo build --offline --locked -p ssg-cli)"
	(cd "$ROOT" && cargo build --offline --locked -q -p ssg-cli --bin "$name" >&2)
	local target=${CARGO_TARGET_DIR:-$ROOT/target}
	case $target in /*) ;; *) target=$ROOT/$target ;; esac
	# A copy: the target directory is shared, another build may replace the file.
	mkdir -p "$W/bin"
	cp "$target/debug/$name" "$W/bin/$name"
	echo "$W/bin/$name"
}

# build <dir> <structure-out|""> [args…]: a fresh site in <dir>/<label>, built by this port into
# <dir>/out; the log goes to <dir>/log.
build() {
	local dir=$1 structure=$2
	shift 2
	rm -rf "$dir"
	mkdir -p "$dir/home" "$dir/cache"
	python3 "$SITES_PY" make "$label" "$dir/$label" --overlay "$overlay" >/dev/null
	python3 "$SITES_PY" cache "$label" "$dir/cache"
	ln -s "$NODE_MODULES" "$dir/$label/node_modules"
	local env=(HOME="$dir/home" TZ=UTC LANG=C.UTF-8
		PATH="/usr/local/bin:/usr/bin:/bin"
		HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 ALL_PROXY=http://127.0.0.1:9
		http_proxy=http://127.0.0.1:9 https_proxy=http://127.0.0.1:9 all_proxy=http://127.0.0.1:9
		NO_PROXY= no_proxy=)
	[ -n "$structure" ] && env+=(FUGO_STRUCTURE_OUT="$structure")
	local start end
	start=$(date +%s)
	if ! (cd "$dir/$label" && env -i "${env[@]}" "$BIN" --clock "$CLOCK" --cacheDir "$dir/cache" "$@" -d "$dir/out" >"$dir/log" 2>&1); then
		log "$label: the build failed ($dir/log):"
		sed -e "s#$dir#\$W#g" "$dir/log" | tail -40 >&2
		exit 1
	fi
	end=$(date +%s)
	log "$label: build $(basename "$dir") in $((end - start)) s"
	grep -E "ERROR|WARN" "$dir/log" | sed -e "s#$dir#\$W#g" -e "s/^/  [rust] /" | head -20 >&2 || true
}

# side <name>: builds both passes (and the structure dump) of one side; sets <name>_min,
# <name>_unmin, <name>_structure to the manifest and dump files.
side() {
	local name=$1
	for pass in $passes; do
		local dir=$W/$name-$pass args=() levels=$unmin_levels full=() structure=
		if [ $pass = minified ]; then
			args=(--minify)
			levels=L1,L4
		else
			full=(--full-text)
			[ -n "$structure_dump" ] && structure=$W/$name.structure.json
		fi
		build "$dir" "$structure" "${args[@]}"
		python3 "$MANIFEST_PY" extract "$dir/out" --project "$dir/$label" --site "$label" \
			--pass $pass --levels $levels "${full[@]}" -o "$W/$name.manifest.$pass.json"
		[ "${KEEP:-}" = 1 ] || rm -rf "$dir"
	done
	eval "${name}_min=\$W/\$name.manifest.minified.json"
	eval "${name}_unmin=\$W/\$name.manifest.unminified.json"
	eval "${name}_structure=\$W/\$name.structure.json"
}

BIN=$(program_binary)
ref_min=$GOLDEN/manifest.minified.json
ref_unmin=$GOLDEN/manifest.unminified.json
ref_structure=$GOLDEN/structure.json
side cand

args=(compare --site "$label" --show "$show" --ref-name golden --ref-unmin "$ref_unmin"
	--cand-name rust --cand-unmin "$cand_unmin"
	--json "$W/structdiff.json" --report "$W/report.txt" --baseline "$BASELINE")
case " $passes " in *" minified "*) args+=(--ref-min "$ref_min" --cand-min "$cand_min") ;; esac
[ -n "$structure_dump" ] && args+=(--ref-structure "$ref_structure" --cand-structure "$cand_structure")
for t in "${tasks[@]}"; do args+=(--task "$t"); done
[ -n "$update" ] && args+=(--update)
[ -n "$report_only" ] && args+=(--report-only)
status=0
python3 "$STRUCTDIFF_PY" "${args[@]}" || status=$?
log "$label: report $W/report.txt, $W/structdiff.json$([ "${KEEP:-}" = 1 ] && echo "; outputs kept in $W")"
exit $status
