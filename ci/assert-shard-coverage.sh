#!/usr/bin/env bash
# ci/assert-shard-coverage.sh — the windows shards ran the whole suite.
#
# Sharding a test leg has exactly one way to go quietly wrong: a target that
# lands in no shard. Three green shards do not rule that out — three greens
# are what a hole looks like. r20's probe (wolf-interp#121) closed it by
# hand, summing 1 + 2 + 47 to the 50 binaries the green macOS job ran on the
# same head. This script does the same thing automatically, every run, and
# in the stronger form: SET equality, so a hole is named rather than merely
# counted.
#
# Three independent sources must agree:
#
#   1. what the windows shards actually ran   (cargo's `Running` lines)
#   2. what the macOS job actually ran        (cargo's `Running` lines)
#   3. what the repository says it has        (`ci/test-shards.sh targets`)
#
# What this prints if the thing it checks HAS failed: the offending target
# names, in the direction they are missing, plus the shard that doubled one.
set -euo pipefail

dir="${1:-artifacts}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mac="$dir/test-binaries-macos-latest/test-binaries.txt"
ubu="$dir/test-binaries-ubuntu-latest/test-binaries.txt"

# Assert non-empty BEFORE asserting equal: two empty files compare equal and
# would print a green that means nothing (wave 44's fifth false signal).
for f in "$mac" "$ubu"; do
  if [ ! -s "$f" ]; then
    echo "::error::$f is missing or empty — there is nothing to compare the shards against"
    exit 1
  fi
done

shopt -s nullglob
shards=("$dir"/test-binaries-windows-shard-*/test-binaries.txt)
shopt -u nullglob
if [ "${#shards[@]}" -ne 3 ]; then
  echo "::error::expected 3 windows shard listings, found ${#shards[@]}"
  printf '  %s\n' "${shards[@]:-<none>}"
  exit 1
fi
for f in "${shards[@]}"; do
  if [ ! -s "$f" ]; then
    echo "::error::$f is empty — a windows shard ran no test binary at all"
    exit 1
  fi
done

cat "${shards[@]}" | sort > "$work/win-all"
sort -u "$work/win-all" > "$work/win"
sort -u "$mac"         > "$work/mac"
sort -u "$ubu"         > "$work/ubu"
ci/test-shards.sh targets | sort -u > "$work/declared"

fail=0

dup=$(uniq -d < "$work/win-all" || true)
if [ -n "$dup" ]; then
  echo "::error::a target is in more than one windows shard — the shards are not a partition:"
  printf '  %s\n' $dup
  fail=1
fi

compare() {  # compare <have> <want> <what>
  local missing extra
  missing=$(comm -13 "$1" "$2")
  extra=$(comm -23 "$1" "$2")
  if [ -n "$missing" ]; then
    echo "::error::$3: these test binaries were NOT run by any windows shard:"
    printf '  %s\n' $missing
    fail=1
  fi
  if [ -n "$extra" ]; then
    echo "::error::$3: these test binaries ran on windows but not there:"
    printf '  %s\n' $extra
    fail=1
  fi
}

compare "$work/win" "$work/mac"      "against the macOS job"
compare "$work/win" "$work/ubu"      "against the ubuntu job"
compare "$work/win" "$work/declared" "against the repository's own target list"

test "$fail" -eq 0

n=$(wc -l < "$work/win" | tr -d ' ')
echo "the three windows shards ran $n test binaries, the same set the macOS and"
echo "ubuntu jobs ran on this head, and the same set ci/test-shards.sh declares."
echo
for f in "${shards[@]}"; do
  printf '  %-56s %3d binaries\n' "$f" "$(wc -l < "$f" | tr -d ' ')"
done
printf '  %-56s %3d binaries\n' "= union" "$n"
