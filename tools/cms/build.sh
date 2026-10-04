#!/bin/sh
# Builds the CMS editor from its sources, crates/cms/web (TypeScript and Sass), into the files
# crates/cms embeds: assets/admin/cms.js, assets/admin/cms.css and assets/worker.js. The
# TypeScript is type-checked first (tsc, strict); then the site generator itself bundles and
# compiles it (js_build, to_css). The libraries (yaml, smol-toml, marked) and tsc come from the
# pinned node modules of tools/dev/node.sh; their licences are in THIRD_PARTY/cms-editor/.
#
#   tools/cms/build.sh [--check] [binary]
#
#   --check   compare the build with the embedded files instead of writing them (exit 1 when
#             they differ: run the script without --check and commit the result)
#   binary    the generator to build with (default: target/release/<name>)
#
# The test `cms::editor_assets_are_built_from_their_sources` of ssg-cli runs the same check.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
mode=write
if [ "${1:-}" = "--check" ]; then
	mode=check
	shift
fi
bin=${1:-}
if [ -z "$bin" ]; then
	name=$(sed -n '/^\[\[bin\]\]/,/^$/s/^name = "\([^"]*\)".*/\1/p' "$root/crates/cli/Cargo.toml")
	bin="$root/target/release/$name"
fi
"$root/tools/dev/node.sh" check >/dev/null || {
	echo "build.sh: run tools/dev/node.sh first (it installs the libraries and tsc)" >&2
	exit 1
}
modules=$("$root/tools/dev/node.sh" path)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp -R "$root/crates/cms/web" "$work/web"
ln -s "$modules" "$work/web/node_modules"

"$modules/.bin/tsc" -p "$work/web"
"$bin" build -s "$work/web" -d "$work/out" --quiet

status=0
for f in admin/cms.js admin/cms.css worker.js; do
	dest="$root/crates/cms/assets/$f"
	if [ "$mode" = check ]; then
		if ! cmp -s "$work/out/$f" "$dest"; then
			echo "build.sh: crates/cms/assets/$f is not what crates/cms/web builds to" >&2
			status=1
		fi
	else
		mkdir -p "$(dirname "$dest")"
		cp "$work/out/$f" "$dest"
		echo "build.sh: wrote crates/cms/assets/$f ($(wc -c <"$dest" | tr -d ' ') bytes)"
	fi
done
exit $status
