#!/usr/bin/env bash
# ci/test-shards.sh — the windows test leg, sharded, with a stable key.
#
# WHY THIS EXISTS (wolf-interp#121). `cargo test` on `windows-latest` is
# 29,267 s — 8 h 08 m, measured at 552786e — against GitHub's 6 h job cap,
# which `timeout-minutes` can lower but never raise. The leg was therefore
# *cancelled*, not failed, on every head carrying is50's and is51's corpus
# growth, and wolf-interp had no windows coverage at all. Three shards fit,
# the widest at 2 h 49 m.
#
# Two properties this file exists to guarantee:
#
#   * THE KEY IS STABLE. A target's shard is a function of its NAME and of
#     nothing else — not of `ls` order, not of the set of targets present in
#     the run, not of anything a run discovers. So a failure names the same
#     shard twice, and a target a later lane adds never moves an existing
#     one onto a different shard.
#
#   * NOTHING FALLS BETWEEN THE SHARDS. The assignment is total: the eight
#     heavy targets are pinned by measurement, and every other target — one
#     that does not exist yet included — lands in a shard by `cksum` of its
#     own name. `ci/assert-shard-coverage.sh` then PROVES in CI that what
#     the shards actually ran is exactly what the macOS job ran on the same
#     head, rather than trusting the table below.
#
# Run from the repository root.
#
# Subcommands:
#   targets            every test binary this crate has, one name per line
#   shard-of NAME      the shard NAME belongs to
#   args N             the `cargo test` arguments for shard N
#   plan               the whole assignment as a table (printed into the log)
#   ran LOGFILE        the test binaries a `cargo test` log says actually RAN
set -euo pipefail

SHARDS=3

# Pinned by measurement at 552786e: windows seconds from the three shards of
# run 35194715878, reconciled against the macOS job of run 35170259143 (whose
# per-target times sum to 15,564 s against an independently taken step total
# of 15,616 s). These eight targets are 29,233 s of the 29,267 s suite; the
# other forty-two are 34 s together, so their placement is noise and the hash
# may have them.
#
#   shard 1  run_corpus 8262 + divergence 1883              = 10,145 s
#   shard 2  doc_truth 5865 + region_machine 2220
#                           + rule_registry 1930            = 10,015 s
#   shard 3  export 4588 + cli 3998 + fuzz_smoke 488        =  9,074 s
#
# The binding constraint is `run_corpus`, which is ONE test binary at 8,262 s
# and grows with the corpus: no target-level split can divide it. This fix
# lasts until `run_corpus` alone passes the cap — about 2.4x today's corpus
# against a 330-minute timeout. Splitting *inside* a target is the next move,
# and it is a different mechanism.
shard_of() {
  case "$1" in
    run_corpus)     echo 1 ;;
    divergence)     echo 1 ;;
    doc_truth)      echo 2 ;;
    region_machine) echo 2 ;;
    rule_registry)  echo 2 ;;
    export)         echo 3 ;;
    cli)            echo 3 ;;
    fuzz_smoke)     echo 3 ;;
    # `cksum` is POSIX and its CRC is the same number on every host we run
    # on (verified: macOS, linux and Git Bash agree), and it depends on the
    # name alone.
    *) echo $(( $(printf '%s' "$1" | cksum | cut -d' ' -f1) % SHARDS + 1 )) ;;
  esac
}

# The crate's test binaries: the lib unit tests, the bin unit tests, and one
# per `tests/*.rs`. `autotests` is on and Cargo.toml declares no `[[test]]`,
# so the directory IS the set — and `assert-shard-coverage.sh` checks that
# claim against what cargo itself reported running, from a different source.
targets() {
  echo lib
  echo bins
  local f
  for f in tests/*.rs; do
    [ -e "$f" ] || continue
    f=${f#tests/}
    echo "${f%.rs}"
  done
}

args() {
  local n="$1" t out=""
  for t in $(targets); do
    [ "$(shard_of "$t")" = "$n" ] || continue
    case "$t" in
      lib)  out="$out --lib" ;;
      bins) out="$out --bins" ;;
      *)    out="$out --test $t" ;;
    esac
  done
  # An empty argument list would make `cargo test` run the WHOLE suite, which
  # is the one failure this file must never have: a shard that looks like it
  # passed quickly while actually being the thing we are sharding.
  if [ -z "$out" ]; then
    echo "::error::shard $n of $SHARDS has no test target; refusing to emit an empty argument list" >&2
    exit 1
  fi
  printf '%s\n' "${out# }"
}

plan() {
  local t
  for t in $(targets); do
    printf '%s\t%s\n' "$(shard_of "$t")" "$t"
  done | sort
}

# What a `cargo test` log says actually ran. Measured from cargo's own
# `Running` lines rather than restated from the table above, so the coverage
# proof has two independent sources. Handles the windows spelling
# (`tests\foo.rs`, CRLF) and colour escapes.
#
# Anchored on `unittests ` and `tests/` so that a `Running `target/debug/lupin
# corpus`` line from a later `cargo run` step can never be counted as a test
# binary.
ran() {
  local esc
  esc=$(printf '\033')
  sed -e "s/${esc}\[[0-9;]*m//g" -e 's/\r$//' "$1" \
    | sed -n -E \
        -e 's#^[[:space:]]*Running unittests src[\\/]lib\.rs .*#lib#p' \
        -e 's#^[[:space:]]*Running unittests src[\\/]main\.rs .*#bins#p' \
        -e 's#^[[:space:]]*Running tests[\\/]([A-Za-z0-9_]+)\.rs .*#\1#p' \
    | sort -u
}

case "${1:-}" in
  targets)  targets | sort ;;
  shard-of) shard_of "$2" ;;
  args)     args "$2" ;;
  plan)     plan ;;
  ran)      ran "$2" ;;
  *) echo "usage: $0 {targets|shard-of NAME|args N|plan|ran LOGFILE}" >&2; exit 2 ;;
esac
