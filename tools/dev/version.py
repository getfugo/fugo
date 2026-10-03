#!/usr/bin/env python3
"""Release versions come from git tags (DEVELOPMENT.md, "CI and releases"; Python 3, stdlib only).

A release is the tag v<major>.<minor>.<patch>[-<pre-release>]; CI builds it with that version
(the compile-time variable $FUGO_BUILD_VERSION, which ssg_base::VERSION reads). `version` in
[workspace.package] of Cargo.toml is only the version of builds not made from a tag, and is
never edited for a release. fugo's releases start at v1.0.0; a v0.x tag never counts as fugo's
(the Go fork's tags v0.1.0 … v0.148.2 were removed, but an old clone may still have them).

Usage:
  version.py next major|minor|patch   the version after the latest release tag of the
                                      repository (`git tag`; pre-releases do not count),
                                      1.0.0 when there is none
  version.py tag <tag>                the version of a release tag (v1.2.3, v1.3.0-rc.1);
                                      exit 1 when it is not one
"""
import re
import subprocess
import sys

TAG_RE = re.compile(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z][0-9A-Za-z.-]*)?")
FIRST = (1, 0, 0)


def parse(tag):
    """(major, minor, patch, pre-release or None) of a release tag of fugo, else None."""
    m = TAG_RE.fullmatch(tag)
    if not m or int(m.group(1)) < FIRST[0]:
        return None
    return int(m.group(1)), int(m.group(2)), int(m.group(3)), m.group(4)


def latest(tags):
    """The highest (major, minor, patch) among the release tags that are not pre-releases."""
    versions = [v[:3] for v in map(parse, tags) if v and v[3] is None]
    return max(versions, default=None)


def bump(current, part):
    """The version after `current` (None: no release yet, so 1.0.0 whatever `part` is)."""
    if current is None:
        return FIRST
    major, minor, patch = current
    if part == "major":
        return major + 1, 0, 0
    if part == "minor":
        return major, minor + 1, 0
    if part == "patch":
        return major, minor, patch + 1
    raise ValueError(f"bump {part!r}: major, minor or patch")


def git_tags():
    out = subprocess.run(["git", "tag", "--list", "v*"], check=True, capture_output=True, text=True)
    return out.stdout.split()


def main(argv):
    if len(argv) == 3 and argv[1] == "next" and argv[2] in ("major", "minor", "patch"):
        tags = git_tags()
        version = "%d.%d.%d" % bump(latest(tags), argv[2])
        if "v" + version in tags:
            sys.exit(f"version.py: tag v{version} exists already")
        print(version)
    elif len(argv) == 3 and argv[1] == "tag":
        v = parse(argv[2])
        if v is None:
            sys.exit(f"version.py: {argv[2]!r} is not a release tag (v<major>.<minor>.<patch>"
                     f"[-<pre-release>], v{'.'.join(map(str, FIRST))} or later)")
        print(argv[2][1:])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
