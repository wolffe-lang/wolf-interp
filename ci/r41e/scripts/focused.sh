#!/usr/bin/env bash
# r41e: the new test target, at the checked-out head.
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/wolf-interp
echo "R41E_FOCUSED_START $(date -u +%FT%TZ) head=$(git rev-parse HEAD) dirty=$(git status --porcelain | wc -l)"
cat /etc/os-release | head -2
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|SigCgt' /proc/self/status
cargo test --locked --test exec_sigpipe_r41e -- --nocapture
echo "R41E_FOCUSED_EXIT $? $(date -u +%FT%TZ)"
