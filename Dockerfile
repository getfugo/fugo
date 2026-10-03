# syntax=docker/dockerfile:1
# The container image of a fugo release: the release's Linux binary (amd64 or arm64) on Debian
# slim, with ca-certificates, git and tzdata (fugo reads time zones from /usr/share/zoneinfo).
# It packages a published release instead of compiling one, so it holds the very binary of the
# release archive and builds in a minute on any platform:
#
#   docker build --build-arg FUGO_VERSION=1.2.3 -t fugo .
#
# .github/workflows/image.yml builds it for linux/amd64 and linux/arm64 and pushes it to
# ghcr.io/getfugo/fugo for every release (DEVELOPMENT.md, "Container image"). Nothing of the
# build context is used (.dockerignore excludes all of it).

ARG DEBIAN=trixie-slim

# The release archive for the target's architecture, checked against the release's checksums
# file. Runs on the build machine's platform, so nothing is emulated.
FROM --platform=$BUILDPLATFORM debian:${DEBIAN} AS release
ARG FUGO_VERSION
# Where the releases are: <FUGO_RELEASES>/v<version>/<file>.
ARG FUGO_RELEASES=https://github.com/getfugo/fugo/releases/download
ARG TARGETARCH
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /release
RUN set -eu; \
    if [ -z "${FUGO_VERSION}" ]; then \
      echo "FUGO_VERSION is not set: docker build --build-arg FUGO_VERSION=<version>, e.g. 1.0.0" >&2; \
      exit 1; \
    fi; \
    archive="fugo_${FUGO_VERSION}_linux-${TARGETARCH}.tar.gz"; \
    sums="fugo_${FUGO_VERSION}_checksums.txt"; \
    curl -fsSLO "${FUGO_RELEASES}/v${FUGO_VERSION}/${archive}"; \
    curl -fsSLO "${FUGO_RELEASES}/v${FUGO_VERSION}/${sums}"; \
    grep "  ${archive}\$" "${sums}" | sha256sum -c -; \
    mkdir doc; \
    tar -xzf "${archive}" -C doc; \
    mv doc/fugo fugo

FROM debian:${DEBIAN}
ARG FUGO_VERSION
LABEL org.opencontainers.image.title="fugo" \
      org.opencontainers.image.description="The fugo static site generator" \
      org.opencontainers.image.source="https://github.com/getfugo/fugo" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.version="${FUGO_VERSION}"

# The user fugo (1000:1000) runs the image; a site mounted from a Linux host is written as the
# host's user with `--user "$(id -u):$(id -g)"`, so /cache is writable by any user and git
# trusts any directory (the mounted checkout belongs to the host's user, not to the container's).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --user-group --create-home --shell /bin/bash fugo \
    && install -d -o fugo -g fugo /src \
    && install -d -m 1777 /cache \
    && git config --system --add safe.directory '*'

COPY --from=release /release/fugo /usr/local/bin/fugo
# The archive's README.md, LICENSE, NOTICE, PROVENANCE.md, THIRD_PARTY_NOTICES.txt and
# THIRD_PARTY/: the licences of what the binary is built from.
COPY --from=release /release/doc/ /usr/share/doc/fugo/

# The binary runs on this base (glibc) and is the version asked for.
RUN set -eu; \
    line=$(fugo version); \
    echo "${line}"; \
    case "${line}" in \
      "fugo v${FUGO_VERSION}-"* | "fugo v${FUGO_VERSION} "*) ;; \
      *) echo "expected fugo v${FUGO_VERSION}" >&2; exit 1 ;; \
    esac

# The caches (`$XDG_CACHE_HOME/fugo_cache`: npm packages, get_remote, images); mount a volume
# here to keep them between runs.
ENV XDG_CACHE_HOME=/cache
USER 1000:1000
WORKDIR /src
# fugo server --bind 0.0.0.0
EXPOSE 1313
ENTRYPOINT ["fugo"]
