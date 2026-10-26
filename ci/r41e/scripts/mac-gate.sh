#!/bin/bash
# r41e: the Mac gate at the release head, behind the 8 GiB floor.
source /Users/mfwolffe/lanes/r41e/mac-env.sh
cd /Users/mfwolffe/lanes/r41e/wolf-interp
log=$1
fails=""
step() { local name=$1; shift; echo "R41E_STEP_START $name $(date -u +%FT%TZ)"; "$@"; local rc=$?; echo "R41E_STEP $name exit=$rc $(date -u +%FT%TZ)"; [ $rc -eq 0 ] || fails="$fails $name"; }
{
  echo "R41E_MAC_GATE_START $(date -u +%FT%TZ) head=$(git rev-parse HEAD) dirty=$(git status --porcelain | wc -l | tr -d ' ')"
  sw_vers; uname -m
  r41e_gate
  step fmt cargo fmt --check
  step clippy cargo clippy --locked -j4 --all-targets -- -D warnings
  r41e_gate
  step tests cargo test --release --locked -j4 \
    --test exec_sigpipe_r41e --test exec_s225 --test mirror_is81 --test variadic_is81 \
    --test build_stamp --test cli --test doc_truth --test conformance --test anchor_admission \
    --test run_corpus --test corpus_harness --test os_process --test proc_fd_s215 --test job_s219 \
    --test spec_extract -- --nocapture
  r41e_gate
  step lib-tests cargo test --release --locked -j4 --lib
  step clean-tree git diff --exit-code
  echo "R41E_MAC_GATE_END $(date -u +%FT%TZ) fails=[${fails# }]"
} > "$log" 2>&1
[ -z "$fails" ]
