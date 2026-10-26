#!/usr/bin/env bash
# r41e: r41's exec control, unchanged but for the lupin binary. r41's tree
# is an overlay mount (/lane): what the run writes there is discarded at
# the container's exit; r41's files are never written.
set -uo pipefail
source /lane/scripts/wolf-env.sh
cd /lane/wolf-lang
printf 'R41E_CONTROL_START %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
for LUPIN in "$@"; do
  export LUPIN
  name=$(basename "$LUPIN")
  "$LUPIN" --version
  sha256sum "$LUPIN"
  cargo test --locked -p wolf_driver --test exec_lanes exec_replaces_the_image_and_keeps_the_pid -- --exact --nocapture > "/r41e/logs/control-$name.log" 2>&1
  rc=$?; printf '%s\n' "$rc" > "/r41e/records/control-$name.exit"
  printf 'R41E_CONTROL lupin=%s exit=%s\n' "$name" "$rc"
done
printf 'R41E_CONTROL_END %s\n' "$(date -u +%FT%TZ)"
