#!/usr/bin/env bash
# r41e on hasu block A: run one inner script in a rootless container.
# Usage: run.sh <phase> <inner script under /lane/scripts> [shim]
set -uo pipefail
base=$HOME/lanes/r41e
phase=$1; inner=$2; shim=${3:-}
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
size=$(du -sk "$base" | awk '{print $1}')
[ "$size" -lt 9500000 ] || { echo "REFUSED lane size ${size} KiB"; exit 3; }
echo "R41E phase=$phase waiting for block A at $(date -u +%FT%TZ), lane=${size} KiB"
(
 flock 9
 echo "R41E phase=$phase acquired block A at $(date -u +%FT%TZ)"
 if [ -n "$shim" ]; then setup='cc -O2 -static /r41-scripts/taskset-shim.c -o /usr/bin/taskset'; else setup='cp /usr/bin/taskset.real /usr/bin/taskset'; fi
 podman run --rm --name "r41e-$phase" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
   --userns keep-id:uid=1000,gid=1000 --user 0 -v "$base:/lane:rw" \
   -v "$HOME/lanes/r41/inputs:/r41-inputs:ro" -v "$HOME/lanes/r41/scripts:/r41-scripts:ro" \
   localhost/r41-toolchain:1 bash -c "$setup"' && /usr/bin/taskset.real -c 0-5 setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash "$0"; exit $?' "/lane/scripts/$inner"
) 9> "$HOME/lanes/hasu-A.lock"
rc=$?
echo "R41E phase=$phase exit=$rc at $(date -u +%FT%TZ)"
exit $rc
