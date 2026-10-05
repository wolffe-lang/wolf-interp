#!/usr/bin/env bash
# ci/place-trace-identity.sh — the place trace never changes a program (is72).
#
# `lupin --trace-places` is a side channel: stdout, the exit code and every
# stderr byte the program (or lupin, for a trap) writes must be the same with
# the flag on and off. This runs EVERY `.lu` file under the vendored corpus
# through the real binary twice — once plain, once with `--trace-places=FILE`
# — each in a fresh empty working directory (an `fs/` program writes where
# it stands; the second run must not see the first's files), stdin closed,
# and compares the three streams byte for byte.
#
# A file whose two runs differ is run plain a second time. If the two PLAIN
# runs differ too, the program is nondeterministic on its own (a clock, a
# pid, a port) and is counted as such, never as a breach; otherwise the
# flag changed it, and the script fails naming the file.
#
# The trace itself is checked for shape on the way: every line it wrote
# starts `{"trace":1,`.
#
# Usage: ci/place-trace-identity.sh LUPIN [CORPUS_DIR]
# Prints the counts and a sha256 digest of the per-file ledger
# (path, exit code, stdout digest, trace line count). Exit 0 = identical.
set -uo pipefail

bin=${1:?usage: ci/place-trace-identity.sh LUPIN [CORPUS_DIR]}
corpus=${2:-vendor/upstream/corpus}
case "$bin" in /*|[A-Za-z]:*) ;; *) bin="$PWD/$bin" ;; esac
case "$corpus" in /*|[A-Za-z]:*) ;; *) corpus="$PWD/$corpus" ;; esac
[ -x "$bin" ] || { echo "::error::no lupin at $bin"; exit 2; }

if command -v sha256sum >/dev/null; then sha() { sha256sum | cut -c1-64; }
else sha() { shasum -a 256 | cut -c1-64; }; fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
ledger="$work/ledger.txt"
: > "$ledger"

# One run: $1 = program, $2 = output prefix, $3 = trace file or empty.
run_one() {
  local cwd
  cwd=$(mktemp -d "$work/cwd.XXXXXX")
  if [ -n "$3" ]; then
    (cd "$cwd" && timeout 120 "$bin" run "--trace-places=$3" "$1" < /dev/null > "$2.out" 2> "$2.err"; echo $? > "$2.rc")
  else
    (cd "$cwd" && timeout 120 "$bin" run "$1" < /dev/null > "$2.out" 2> "$2.err"; echo $? > "$2.rc")
  fi
  rm -rf "$cwd"
}

same() { cmp -s "$1.out" "$2.out" && cmp -s "$1.err" "$2.err" && cmp -s "$1.rc" "$2.rc"; }

total=0; identical=0; nondet=0; breached=0; lines=0; badshape=0
while IFS= read -r rel; do
  total=$((total + 1))
  file="$corpus/$rel"
  run_one "$file" "$work/off" ""
  run_one "$file" "$work/on" "$work/trace.jsonl"
  n=0
  if [ -f "$work/trace.jsonl" ]; then
    n=$(wc -l < "$work/trace.jsonl" | tr -d ' ')
    if grep -qv '^{"trace":1,' "$work/trace.jsonl"; then
      badshape=$((badshape + 1)); echo "BAD SHAPE  $rel"
    fi
    rm -f "$work/trace.jsonl"
  fi
  lines=$((lines + n))
  if same "$work/off" "$work/on"; then
    identical=$((identical + 1))
  else
    run_one "$file" "$work/again" ""
    if same "$work/off" "$work/again"; then
      breached=$((breached + 1))
      echo "::error::the place trace changed $rel"
      diff "$work/off.out" "$work/on.out" | head -5
      diff "$work/off.err" "$work/on.err" | head -5
      echo "rc off=$(cat "$work/off.rc") on=$(cat "$work/on.rc")"
    else
      nondet=$((nondet + 1)); echo "NONDETERMINISTIC (plain twice differs)  $rel"
    fi
  fi
  printf '%s %s %s %s\n' "$rel" "$(cat "$work/off.rc")" "$(sha < "$work/off.out")" "$n" >> "$ledger"
done < <(cd "$corpus" && find . -name '*.lu' | sed 's#^\./##' | LC_ALL=C sort)

echo "place-trace identity: $total files, $identical identical, $nondet nondeterministic on their own, $breached changed by the trace, $badshape malformed traces; $lines trace lines"
echo "ledger sha256 $(sha < "$ledger")"
[ "$total" -gt 900 ] || { echo "::error::only $total corpus files — the walk went dark"; exit 1; }
[ "$breached" = 0 ] && [ "$badshape" = 0 ]
