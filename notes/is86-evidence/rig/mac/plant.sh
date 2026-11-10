#!/bin/bash
# is86 plant take, under the org's recorder, in a worktree ON the plant
# branch: the build, the four controls (must stay green), then each planted
# row on its own; a red row prints `FAIL <row> exit=<rc>`.
set -u
T=session_s290
CONTROLS="well_formed_calls_are_refused_by_name_on_this_host windows_answers_the_unsupported_row_after_the_shape every_unix_target_and_the_bundle_refuse_by_name the_declared_rows_discriminate_a_handler_s_arms"
ROWS="the_spawn_s_shape_is_invalid_on_this_host the_spawn_s_shape_precedes_the_host_on_every_target the_corpus_rows_stop_at_the_forged_source_spawn_by_name the_corpus_rows_print_the_same_four_lines_in_process"
step_start() { printf 'START %s\n' "$1"; s=$SECONDS; }
step_end() { printf 'END %s exit=%s secs=%s\n' "$1" "$2" "$((SECONDS-s))"; }
step_start plant-head
{ git rev-parse HEAD; git log --oneline -2; git diff --stat HEAD^ HEAD; git diff HEAD^ HEAD; } 2>&1 | sed 's/^/  /'
step_end plant-head 0
step_start plant-build
cargo test --locked --test $T --no-run 2>&1 | sed 's/^/  /'
rc=${PIPESTATUS[0]}
step_end plant-build "$rc"
[ "$rc" = 0 ] || exit "$rc"
step_start plant-controls
worst=0
for c in $CONTROLS; do
  cargo test --locked --test $T "$c" -- --exact --test-threads=2 2>&1 | sed 's/^/  /'
  r=${PIPESTATUS[0]}
  [ "$r" = 0 ] || { echo "  CONTROL-RED $c exit=$r"; worst=$r; }
done
step_end plant-controls "$worst"
[ "$worst" = 0 ] || exit "$worst"
step_start plant-rows
worst=0
for row in $ROWS; do
  cargo test --locked --test $T "$row" -- --exact --test-threads=2 2>&1 | sed 's/^/  /'
  r=${PIPESTATUS[0]}
  if [ "$r" != 0 ]; then echo "FAIL $row exit=$r"; worst=$r; else echo "  PASS $row"; fi
done
step_end plant-rows "$worst"
exit "$worst"
