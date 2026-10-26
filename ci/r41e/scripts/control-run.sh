#!/usr/bin/env bash
# r41e: r41's wolf-lang container (wolf-container.sh's flags), r41's tree as
# an overlay, this lane's directory read-write at /r41e. Args: lupin paths
# as the container sees them.
set -uo pipefail
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
echo "R41E phase=control waiting for block A at $(date -u +%FT%TZ)"
(
 flock 9
 echo "R41E phase=control acquired block A at $(date -u +%FT%TZ)"
 podman run --rm --name r41e-control --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
   --userns keep-id:uid=1000,gid=1000 --user 0 -v "$HOME/lanes/r41:/lane:O" -v "$HOME/lanes/r41e:/r41e:rw" \
   -w /lane/wolf-lang localhost/r41-toolchain:1 bash -c 'cc -O2 -static /lane/scripts/taskset-shim.c -o /usr/bin/taskset; /usr/bin/taskset.real -c 0-5 setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /r41e/scripts/control.sh "$@"; exit $?' bash "$@"
) 9> "$HOME/lanes/hasu-A.lock"
rc=$?
echo "R41E phase=control exit=$rc at $(date -u +%FT%TZ)"
exit $rc
