#!/usr/bin/env bash
# ci/test-shards-test.sh — the shard key's own test (is85). bash and coreutils.
#
#   S1  each heavy target sits in the shard its measurement pins it to
#       (ci/test-shards.sh's table; ci/is85.md for the windows numbers that
#       moved region_machine beside run_corpus);
#   S2  every declared target lands in exactly one of the three shards, and
#       each shard's `cargo test` arguments name exactly its own targets;
#   S3  exactly one shard runs the doc-tests.
#
# Prints a PASS or FAIL line per check and exits 1 on any FAIL.
set -u
cd "$(dirname "$0")/.." || exit 1
fails=0
say() {
  if [ "$1" = 0 ]; then echo "test-shards-test: PASS $2"; else echo "test-shards-test: FAIL $2"; fails=$((fails + 1)); fi
}

# S1: the pinned heavy targets.
for pin in run_corpus:1 divergence:1 region_machine:1 doc_truth:2 rule_registry:2 export:3 cli:3 fuzz_smoke:3 doc:3; do
  t=${pin%%:*}; want=${pin##*:}
  got=$(bash ci/test-shards.sh shard-of "$t")
  [ "$got" = "$want" ]; say $? "S1 $t is in shard $want (got $got)"
done

# S2: a partition, by the key and by the arguments it emits.
bad=""
for t in $(bash ci/test-shards.sh targets); do
  n=$(bash ci/test-shards.sh shard-of "$t")
  case "$n" in 1|2|3) ;; *) bad="$bad $t=$n" ;; esac
done
[ -z "$bad" ]; say $? "S2 every target's shard is 1, 2 or 3${bad:+ (not:$bad)}"
emitted=$(for n in 1 2 3; do bash ci/test-shards.sh args "$n" | tr ' ' '\n'; done | grep -v '^--test$' | grep -v '^$' \
  | sed -e 's/^--lib$/lib/' -e 's/^--bins$/bins/' | sort)
declared=$(bash ci/test-shards.sh targets | grep -vx doc | sort)
[ "$emitted" = "$declared" ]; say $? "S2 the three shards' arguments name every target but the doc-tests once ($(printf '%s\n' "$emitted" | wc -l | tr -d ' ') of $(printf '%s\n' "$declared" | wc -l | tr -d ' '))"

# S3: the doc-tests.
owners=0
for n in 1 2 3; do bash ci/test-shards.sh has-doc "$n" && owners=$((owners + 1)); done
[ "$owners" = 1 ]; say $? "S3 exactly one shard runs the doc-tests ($owners)"

if [ "$fails" -ne 0 ]; then echo "test-shards-test: FAIL ($fails)"; exit 1; fi
echo "test-shards-test: every check passes"
