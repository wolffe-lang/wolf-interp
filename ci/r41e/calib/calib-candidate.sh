#!/usr/bin/env bash
set -uo pipefail
base=$HOME/lanes/r41e
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
(
 flock 9
 echo "R41E acquired block A at $(date -u +%FT%TZ)"
 podman run --rm --name r41e-calib-candidate --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
   --userns keep-id:uid=1000,gid=1000 --user 0 -v "$base:/lane:rw" \
   localhost/r41-toolchain:1 bash -c '/usr/bin/taskset.real -c 0-5 setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/calib/calib-candidate-inner.sh "$@"; exit $?' bash "$@"
) 9> "$HOME/lanes/hasu-A.lock"
