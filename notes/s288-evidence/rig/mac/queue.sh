#!/bin/bash
# s288: queue one take at a commit, detached (macOS has no setsid: nohup and
# disown); log, pid and exit marker under records/. Adapted from s285's
# queue-interp-take.sh.
#   queue.sh NAME REPO SHA SCRIPT [ENV=VALUE...]
set -eu
export LANE=$LANE
name=$1; repo=$2; sha=$3; script=$4; shift 4
short=$(git -C "$LANE/$repo" rev-parse --short=7 "$sha^{commit}")
R=$LANE/records
log=$R/$name-$short.log
mark=$R/$name-$short.exit
test ! -e "$log"
cd "$LANE"
nohup env "$@" bash -c '"$LANE/rig/at-commit.sh" "$1" "$2" bash "$3"; echo $? > "$4"' bash "$repo" "$short" "$script" "$mark" > "$log" 2>&1 &
pid=$!
disown
echo "$pid" > "$R/$name-$short.pid"
echo "queued pid $pid log $log marker $mark"
