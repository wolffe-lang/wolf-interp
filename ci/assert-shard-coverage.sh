#!/usr/bin/env bash
# ci/assert-shard-coverage.sh — every OS's shards ran the whole suite, once.
#
# Sharding a test leg has exactly one way to go quietly wrong: a target that
# lands in no shard. Three green shards do not rule that out — three greens
# are what a hole looks like. r20's probe (wolf-interp#121) closed it by
# hand, summing 1 + 2 + 47 to the 50 binaries the green macOS job ran on the
# same head; is52 made that automatic for windows against the unsharded
# linux and macOS jobs. Since is57 (wolf-interp#123) no OS runs an unsharded
# job, so the reference is the repository's own target list, and every OS
# is held to it separately.
#
# For each of ubuntu, macos and windows (or the OSes SHARD_OSES names):
#
#   1. exactly three shard listings, none empty;
#   2. no test binary in two shards (the shards are a partition);
#   3. the union of what the shards' `cargo test` REPORTED running (cargo's
#      `Running`/`Doc-tests` lines) equals `ci/test-shards.sh targets`, both
#      directions;
#   4. per test: no test counted by two shards, and every test's binary is
#      one its own shard ran.
#
# What this prints if the thing it checks HAS failed: the offending target
# or test names, in the direction they are missing, and the shard that
# doubled one. `self-test DIR` plants a hole and a double in copies of DIR
# and exits 0 only if the clean set is green and both plants are refused by
# name — the gate seen red on every run, not once.
set -euo pipefail

# The OSes whose shards this run carries. Every trunk push, nightly and
# dispatch run, and a PR labelled `full-matrix`, shards all three; a plain PR
# run shards ubuntu alone (is75: windows and macOS run a smoke there, which
# is not a partition and is not checked here). The workflow says which with
# SHARD_OSES; unset means all three, so a local run checks everything.
OSES="${SHARD_OSES:-ubuntu macos windows}"
if [ -z "${OSES// /}" ]; then
  echo "::error::SHARD_OSES is set but names no OS"; exit 1
fi
base="$(mktemp -d)"
trap 'rm -rf "$base"' EXIT

check() {
  local dir="$1" work os f d dup stray fail=0
  local -a shards
  work="$(mktemp -d "$base/check.XXXXXX")"

  ci/test-shards.sh targets | LC_ALL=C sort -u > "$work/declared"
  if [ ! -s "$work/declared" ]; then
    echo "::error::ci/test-shards.sh declared no targets"
    return 1
  fi

  for os in $OSES; do
    shopt -s nullglob
    shards=("$dir"/test-binaries-"$os"-shard-*/test-binaries.txt)
    shopt -u nullglob
    if [ "${#shards[@]}" -ne 3 ]; then
      echo "::error::$os: expected 3 shard listings, found ${#shards[@]}"
      printf '  %s\n' "${shards[@]:-<none>}"
      fail=1
      continue
    fi
    # Assert non-empty BEFORE asserting equal: two empty files compare equal
    # and would print a green that means nothing (wave 44's fifth false signal).
    local empty=0
    for f in "${shards[@]}"; do
      d=$(dirname "$f")
      if [ ! -s "$f" ]; then
        echo "::error::$f is empty — a shard ran no test binary at all"; empty=1
      fi
      if [ ! -s "$d/test-names.txt" ]; then
        echo "::error::$d/test-names.txt is missing or empty — a shard ran no test at all"; empty=1
      fi
    done
    if [ "$empty" -ne 0 ]; then fail=1; continue; fi

    cat "${shards[@]}" | LC_ALL=C sort > "$work/$os-all"
    LC_ALL=C sort -u "$work/$os-all" > "$work/$os"
    dup=$(LC_ALL=C uniq -d < "$work/$os-all" || true)
    if [ -n "$dup" ]; then
      echo "::error::$os: a target is in more than one shard — the shards are not a partition:"
      printf '  %s\n' $dup
      fail=1
    fi
    local missing extra
    missing=$(LC_ALL=C comm -13 "$work/$os" "$work/declared")
    extra=$(LC_ALL=C comm -23 "$work/$os" "$work/declared")
    if [ -n "$missing" ]; then
      echo "::error::$os: these declared test binaries were NOT run by any shard:"
      printf '  %s\n' $missing
      fail=1
    fi
    if [ -n "$extra" ]; then
      echo "::error::$os: these test binaries ran in a shard but ci/test-shards.sh does not declare them:"
      printf '  %s\n' $extra
      fail=1
    fi

    # Per test. A test's identity is binary + name; the outcome column is not
    # part of it.
    : > "$work/$os-tests-all"
    for f in "${shards[@]}"; do
      d=$(dirname "$f")
      cut -f1,2 "$d/test-names.txt" >> "$work/$os-tests-all"
      stray=$(cut -f1 "$d/test-names.txt" | LC_ALL=C sort -u \
                | LC_ALL=C comm -23 - <(LC_ALL=C sort -u "$f"))
      if [ -n "$stray" ]; then
        echo "::error::$d: tests listed under binaries this shard did not run:"
        printf '  %s\n' $stray
        fail=1
      fi
    done
    LC_ALL=C sort "$work/$os-tests-all" > "$work/$os-tests"
    dup=$(LC_ALL=C uniq -d < "$work/$os-tests" || true)
    if [ -n "$dup" ]; then
      echo "::error::$os: a test ran in more than one shard:"
      printf '%s\n' "$dup" | sed 's/^/  /'
      fail=1
    fi
  done

  [ "$fail" -eq 0 ] || return 1

  echo "each OS checked ($OSES): its three shards ran the $(wc -l < "$work/declared" | tr -d ' ') test binaries"
  echo "ci/test-shards.sh declares, each in exactly one shard, and no test in two."
  echo
  for os in $OSES; do
    for f in "$dir"/test-binaries-"$os"-shard-*/test-binaries.txt; do
      printf '  %-52s %3d binaries %5d tests\n' "$f" \
        "$(wc -l < "$f" | tr -d ' ')" \
        "$(wc -l < "$(dirname "$f")/test-names.txt" | tr -d ' ')"
    done
    printf '  %-52s %3d binaries %5d tests\n' "= $os" \
      "$(wc -l < "$work/$os" | tr -d ' ')" \
      "$(wc -l < "$work/$os-tests" | tr -d ' ')"
  done
  # Informational, not a gate: tests behind a `#[cfg]` differ by OS.
  echo
  for os in macos windows; do
    case " $OSES " in *" $os "*) ;; *) continue ;; esac
    case " $OSES " in *" ubuntu "*) ;; *) continue ;; esac
    echo "per-test, ubuntu against $os (cfg-gated tests; informational):"
    LC_ALL=C comm -23 "$work/ubuntu-tests" "$work/$os-tests" | sed 's/^/  ubuntu only: /'
    LC_ALL=C comm -13 "$work/ubuntu-tests" "$work/$os-tests" | sed "s/^/  $os only: /"
  done
}

# The plants' refusals are printed with the `::error::` prefix defused: on a
# runner that prefix is a workflow command, and a green job carrying failure
# annotations is exactly what a swallowed red looks like (lane-audit.sh
# reads annotations, not logs). A self-test that FAILS prints its evidence
# raw, so a real red still annotates.
planted() {
  sed 's/::error::/(planted, refused as expected) /'
}

self_test() {
  local dir="$1" p out f victim test1
  p="$(mktemp -d "$base/self.XXXXXX")"

  # The clean set must be green, or the plants below prove nothing.
  if ! out=$(check "$dir" 2>&1); then
    printf '%s\n' "$out"; echo "::error::self-test: the unplanted set is not green"; return 1
  fi

  # Plant 1, a hole: drop one binary, and its tests, from ubuntu shard 2.
  cp -R "$dir" "$p/hole"
  f="$p/hole/test-binaries-ubuntu-shard-2/test-binaries.txt"
  victim=$(head -1 "$f")
  if [ -z "$victim" ]; then echo "::error::self-test: nothing to plant"; return 1; fi
  grep -vxF "$victim" "$f" > "$p/b.new"; mv "$p/b.new" "$f"
  f="$p/hole/test-binaries-ubuntu-shard-2/test-names.txt"
  awk -F'\t' -v v="$victim" '$1 != v' "$f" > "$p/n.new"; mv "$p/n.new" "$f"
  if out=$(check "$p/hole" 2>&1); then
    printf '%s\n' "$out"; echo "::error::self-test: a planted hole ($victim) was not refused"; return 1
  fi
  if ! printf '%s\n' "$out" | grep -qxF "  $victim"; then
    printf '%s\n' "$out"; echo "::error::self-test: the hole was refused without naming $victim"; return 1
  fi
  echo "self-test: planted hole '$victim' (ubuntu shard 2) refused by name:"
  printf '%s\n' "$out" | grep -A1 'NOT run' | planted | sed 's/^/    /'

  # Plant 2, a double: macOS shard 1's first test also listed by shard 3
  # (the first OS checked when this run carries no macOS shards).
  local dos=macos
  case " $OSES " in *" macos "*) ;; *) dos=$(set -- $OSES; echo "$1") ;; esac
  cp -R "$dir" "$p/double"
  test1=$(head -1 "$p/double/test-binaries-$dos-shard-1/test-names.txt")
  printf '%s\n' "$test1" >> "$p/double/test-binaries-$dos-shard-3/test-names.txt"
  printf '%s\n' "${test1%%	*}" >> "$p/double/test-binaries-$dos-shard-3/test-binaries.txt"
  if out=$(check "$p/double" 2>&1); then
    printf '%s\n' "$out"; echo "::error::self-test: a planted double was not refused"; return 1
  fi
  if ! printf '%s\n' "$out" | grep -q 'more than one shard'; then
    printf '%s\n' "$out"; echo "::error::self-test: the double was refused for another reason"; return 1
  fi
  echo "self-test: planted double ($dos shards 1 and 3) refused:"
  printf '%s\n' "$out" | grep -A1 'more than one shard' | planted | sed 's/^/    /'
  echo "self-test: the clean set is green and both plants are red."
}

case "${1:-}" in
  self-test) self_test "${2:-artifacts}" ;;
  *)         check "${1:-artifacts}" ;;
esac
