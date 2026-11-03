#!/usr/bin/env bash
# Root child of the deadline-bounded, reaping container PID 1; never PID 1 itself.
set -uo pipefail
cc -O2 -static ci/r42/taskset-shim.c -o /usr/bin/taskset || exit $?
/usr/bin/taskset.real -c "${R42_CPU_FOUR:?}" setpriv \
    --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all \
    --inh-caps=-all --ambient-caps=-all --no-new-privs \
    setsid --fork --wait bash -c 'bash "$@"; exit $?' bash \
        ci/r42/gauntlet.sh "$@"
rc=$?
exit "$rc"
