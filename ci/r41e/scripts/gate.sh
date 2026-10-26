#!/usr/bin/env bash
# r41e: the lupin 0.1.58 gate on hasu block A (r41's lupin-gate shape, plus
# the conformance, anchors and corpus targets and a published-0.1.57 corpus
# comparison). Every step runs; each step's exit is printed.
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/wolf-interp
export WOLF_INTERP_STAMP_NONCE="$(git rev-parse HEAD)"
step() { local name=$1; shift; echo "R41E_STEP_START $name $(date -u +%FT%TZ)"; "$@"; local rc=$?; echo "R41E_STEP $name exit=$rc $(date -u +%FT%TZ)"; [ $rc -eq 0 ] || fails="$fails $name"; }
fails=""
echo "R41E_GATE_START $(date -u +%FT%TZ) head=$(git rev-parse HEAD) dirty=$(git status --porcelain | wc -l)"
head -2 /etc/os-release
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|SigCgt' /proc/self/status
rustc --version; cargo --version
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step tiers-test python3 ci/tiers-test.py
step dist-test python3 ci/is80/dist-test.py
step anchors python3 ci/is81-variadic/anchors.py
step build-release cargo build --release --locked
target/release/lupin --version
sha256sum target/release/lupin; sha256sum target/release/lupin
cp target/release/lupin /lane/records/lupin-candidate-$(git rev-parse --short HEAD)
pub=/r41-inputs/lupin057/lupin-0.1.57-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.57-x86_64-unknown-linux-gnu/lupin
"$pub" --version
step corpus-candidate bash -c 'target/release/lupin corpus --json > /lane/records/corpus-candidate.json'
step corpus-0.1.57 bash -c "$pub corpus --json > /lane/records/corpus-0.1.57.json"
step corpus-compare python3 -I /lane/scripts/corpus-compare.py /lane/records/corpus-0.1.57.json /lane/records/corpus-candidate.json
step tests cargo test --release --locked \
  --test exec_sigpipe_r41e --test exec_s225 --test mirror_is81 --test variadic_is81 \
  --test build_stamp --test cli --test doc_truth --test conformance --test anchor_admission \
  --test run_corpus --test corpus_harness --test export --test os_process --test proc_fd_s215 \
  --test job_s219 --test job_wait_s242 --test group_s241 --test signal_shell_s232 --test signal_one_is77 \
  --test spec_extract -- --nocapture
step lib-tests cargo test --release --locked --lib
step fmt-again cargo fmt --check
step clean-tree git diff --exit-code
echo "R41E_GATE_END $(date -u +%FT%TZ) fails=[${fails# }]"
[ -z "$fails" ]
