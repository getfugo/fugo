#!/bin/sh
# Release versions come from git tags (DEVELOPMENT.md, "CI and releases").
#
# A release is the tag v<major>.<minor>.<patch>[-<pre-release>]; CI builds it with that version
# (the compile-time variable $FUGO_BUILD_VERSION, which ssg_base::VERSION reads). `version` in
# [workspace.package] of Cargo.toml is only the version of builds not made from a tag, and is
# never edited for a release. Releases start at v1.0.0; a v0.x tag never counts (the Go fork's
# tags v0.1.0 … v0.148.2 were removed, but an old clone may still have them).
#
#   tools/dev/version.sh next major|minor|patch   the version after the latest release tag of
#                                                 the repository (`git tag`; pre-releases do not
#                                                 count), 1.0.0 when there is none
#   tools/dev/version.sh tag <tag>                the version of a release tag (v1.2.3,
#                                                 v1.3.0-rc.1); exit 1 when it is not one
set -eu

usage() {
	sed -n '2,/^set -eu/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//' >&2
	exit 2
}

# is_release <tag>: a release tag of v1.0.0 or later.
is_release() {
	case $1 in
	*[!0-9A-Za-z.-]* | "") return 1 ;;
	esac
	printf '%s\n' "$1" |
		grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z][0-9A-Za-z.-]*)?$' &&
		[ "${1%%.*}" != v0 ]
}

# The versions of the releases that are not pre-releases.
releases() {
	git tag --list 'v*' | while read -r t; do
		if is_release "$t"; then
			case $t in
			*-*) ;;
			*) echo "${t#v}" ;;
			esac
		fi
	done
}

case ${1:-} in
next)
	[ $# -eq 2 ] || usage
	case $2 in major | minor | patch) ;; *) usage ;; esac
	latest=$(releases | sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1)
	if [ -z "$latest" ]; then
		next=1.0.0
	else
		major=${latest%%.*}
		rest=${latest#*.}
		minor=${rest%%.*}
		patch=${rest#*.}
		case $2 in
		major) next=$((major + 1)).0.0 ;;
		minor) next=$major.$((minor + 1)).0 ;;
		patch) next=$major.$minor.$((patch + 1)) ;;
		esac
	fi
	if [ -n "$(git tag --list "v$next")" ]; then
		echo "version.sh: tag v$next exists already" >&2
		exit 1
	fi
	echo "$next"
	;;
tag)
	[ $# -eq 2 ] || usage
	if ! is_release "$2"; then
		echo "version.sh: '$2' is not a release tag (v<major>.<minor>.<patch>[-<pre-release>], v1.0.0 or later)" >&2
		exit 1
	fi
	echo "${2#v}"
	;;
*) usage ;;
esac
