#!/usr/bin/env bash
# ci/glibc-floor.sh BINARY [FLOOR_MINOR] — the linux archive starts on the
# oldest glibc the project states (wolf-lang#447, the lupin half).
#
# lupin 0.1.38's x86-64 and aarch64 archives were built on ubuntu-latest
# (24.04) and imported GLIBC_2.39 through two WEAK symbols Rust std's spawn
# path links when the build host's libc defines them (`pidfd_spawnp`,
# `pidfd_getpid`). Weak or not, the loader refuses a binary whose version
# requirement names a node the host libc lacks, so the archive did not start
# on Ubuntu 22.04 LTS (glibc 2.35) at all. wolf 0.2.16 fixed its half by
# building on 22.04 and measuring the floor on the shipped binary; this is the
# same measurement, one script so the release job and the CI job cannot drift.
#
# It measures the BINARY, never the runner: the highest `GLIBC_x.y` version
# `objdump -T` names. The floor is 2.35 (Ubuntu 22.04, Debian 12) unless a
# second argument names another minor.
#
# What this prints if the thing it checks HAS failed: the offending version,
# every symbol that asks for more than the floor, and exit 1. An objdump that
# names no GLIBC version at all is also exit 1 — a probe that saw nothing is
# not a pass.
set -euo pipefail

bin=${1:?usage: ci/glibc-floor.sh BINARY [FLOOR_MINOR]}
floor=${2:-35}

test -f "$bin" || { echo "::error::no binary at $bin"; exit 1; }
# `sed -n 1p` reads to the end: `head -1` would close the pipe early, and
# under pipefail the SIGPIPE it hands `ldd` fails the substitution.
echo "host libc: $(ldd --version 2>&1 | sed -n 1p)"
# `|| true` inside the group: a binary naming no GLIBC version must reach the
# loud error below, not die silently on grep's exit 1 under `set -e`.
versions=$(objdump -T "$bin" | { grep -o 'GLIBC_[0-9.]*' || true; } | sort -uV)
need=$(printf '%s\n' "$versions" | tail -1)
test -n "$need" || { echo "::error::objdump -T named no GLIBC version in $bin — the probe saw nothing"; exit 1; }
echo "highest GLIBC symbol version needed: $need"
minor=${need#GLIBC_2.}
minor=${minor%%.*}
if [ "$minor" -gt "$floor" ]; then
  echo "::error::$need is above the 2.$floor floor (wolf-lang#447) — this binary will not start on a glibc 2.$floor host"
  objdump -T "$bin" | awk -v floor="$floor" '
    match($0, /GLIBC_2\.[0-9]+/) {
      v = substr($0, RSTART + 8, RLENGTH - 8) + 0
      if (v > floor) print "  needs " substr($0, RSTART, RLENGTH) ": " $NF
    }'
  exit 1
fi
echo "floor held: $need <= GLIBC_2.$floor"
