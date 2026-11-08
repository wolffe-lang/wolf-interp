#!/bin/bash
# s285: queue one Mac interp take at a commit, detached; log and exit marker
# under records/.
#   queue-interp-take.sh NAME SHA [ENV=VALUE...]
set -eu
export LANE=$LANE
name=$1; sha=$2; shift 2
L=$LANE/records
log=$L/mac-interp-$name-$sha.log
mark=$L/mac-interp-$name-$sha.exit
test ! -e "$log"
cd "$LANE"
nohup env "$@" bash -c '"$LANE/rig/build-at-interp.sh" "$1" bash "$LANE/rig/mac-interp.sh"; echo $? > "$2"' bash "$sha" "$mark" > "$log" 2>&1 &
disown
echo "queued pid $! log $log marker $mark"
