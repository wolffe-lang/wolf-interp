#!/bin/bash
# r41e: one focused cargo test on the Mac behind the 8 GiB gate. Args: log, then cargo test args.
source /Users/mfwolffe/lanes/r41e/mac-env.sh
log=$1; shift
cd /Users/mfwolffe/lanes/r41e/wolf-interp
{
  r41e_gate
  echo "R41E_MAC_START $(date -u +%FT%TZ) head=$(git rev-parse HEAD) dirty=$(git status --porcelain | wc -l | tr -d ' ')"
  cargo test --locked -j4 "$@"
  rc=$?
  echo "R41E_MAC_EXIT $rc $(date -u +%FT%TZ)"
} > "$log" 2>&1
