#!/usr/bin/env bash
# li-wait: one development take inside the toolchain container, on the
# block's CPUs (take.sh holds the locks). Every step prints START and END
# lines and its output indented two spaces.
#   take-box.sh red | green
set -uo pipefail
export PATH=/opt/s263-rust/bin:$PATH
step() {
  local n=$1; shift
  local s=$SECONDS
  echo "START $n"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  echo "END $n exit=$rc secs=$((SECONDS - s))"
  return "$rc"
}
step head bash -c 'git rev-parse HEAD; git log --oneline -1; rustc -V; cargo -V; nproc; uname -r; grep -E "^Sig(Blk|Ign):" /proc/self/status; env | grep ^CARGO_PROFILE | sort'
step fetch cargo fetch --locked || exit $?
step build cargo test --locked -j4 --test ready_s290 --no-run || exit $?
case "$1" in
  red)
    step rows cargo test --locked -j4 --test ready_s290 -- --test-threads=4
    ;;
  green)
    step rows cargo test --locked -j4 --test ready_s290 -- --test-threads=4
    step fmt cargo fmt --all --check
    step clippy cargo clippy --locked -j4 --all-targets -- -D warnings
    step lib cargo test --locked -j4 --lib -- --test-threads=6
    step neighbours cargo test --locked -j4 --test net_unix --test net_family --test net_s141 --test net_error_is76 --test net_accept_race --test os_mirror_is83 --test os_mirror_target_is83 --test session_s290 --test pty_s292 --test fs_family -- --test-threads=4
    ;;
esac
echo "TAKE-BOX done $(date -u +%FT%TZ)"
