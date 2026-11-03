#!/usr/bin/env bash
# Linux-only launcher. Stage each named commit as a worktree first.
# Detach this launcher with setsid -f; it owns pid/log/exit records.
# Args: debug|release, full candidate head, optional full plant head.
set -uo pipefail
profile=${1:?debug or release}
head=${2:?full candidate head}
plant_head=${3:-}
base=${R42_ROOT:-/home/mfwolffe/lanes/r42}
case "$profile" in debug|release) ;; *) exit 2 ;; esac
[[ "$head" =~ ^[0-9a-f]{40}$ ]] || exit 2
name="$profile-$head"
mkdir -p "$base/records/$name" "$base/scratch/home" "$base/scratch/tmp" \
    "$base/scratch/cache" "$base/cargo"
echo $$ > "$base/records/$name.pid"
marker="$base/records/$name.exit"
if [[ -e "$marker" ]]; then echo 'Refuse overwriting an existing run marker'; exit 2; fi
snapshot="$base/builds/$name"
if [[ ! -f "$snapshot/ci/r42/gauntlet.sh" ]]; then echo "Missing snapshot $snapshot"; exit 2; fi
if [[ -n "$plant_head" ]]; then
    [[ "$plant_head" =~ ^[0-9a-f]{40}$ ]] || exit 2
    plant="/lane/builds/plant-$plant_head"
else
    plant=""
fi
size=$(du -sk "$base" | awk '{print $1}')
if (( size >= 9500000 )); then echo "Lane budget refused: ${size} KiB"; exit 3; fi
export PATH=/home/mfwolffe/lanes/hasu-env/bin:/home/mfwolffe/lanes/hasu-env/profile/bin:$PATH
a=$(ps -eo args | grep -c '[f]lock .*hasu-block-A.lock' || true)
b=$(ps -eo args | grep -c '[f]lock .*hasu-block-B.lock' || true)
block=A; four=0-3; one=0
if (( b < a )); then block=B; four=6-9; one=6; fi
echo "SUBMIT $name waiters_A=$a waiters_B=$b block=$block cpus=$four size_KiB=$size $(date -u +%FT%TZ)"
# timeout's foreground mode signals its own podman child, never a process group.
# The 7000s cap leaves teardown time below the two-hour lock-hold ceiling.
flock -w 21600 "/home/mfwolffe/lanes/hasu-block-$block.lock" \
    flock -w 21600 "/home/mfwolffe/lanes/hasu-$block.lock" \
    timeout --foreground 7000 podman run --rm --name "r42-$profile-${head:0:12}" \
    --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
    --stop-timeout 30 --userns keep-id:uid=1000,gid=1000 --user 0 \
    -v "$base:/lane:rw" -v "$base:$base:rw" -w "/lane/builds/$name" \
    -e PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    -e RUSTUP_HOME=/opt/s263-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
    -e CARGO_HOME=/lane/cargo -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 \
    -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 \
    -e CARGO_PROFILE_RELEASE_DEBUG=0 -e RUST_TEST_THREADS=4 \
    -e HOME=/lane/scratch/home -e TMPDIR=/lane/scratch/tmp \
    -e XDG_CACHE_HOME=/lane/scratch/cache -e LUPIN_SIGNAL_TESTS=1 \
    -e "R42_CPU_FOUR=$four" -e "R42_CPU_ONE=$one" \
    -e "R42_TASKSET_LOG=/lane/records/$name/taskset-translations.log" \
    localhost/r41-toolchain:1 bash -c '
        unset CARGO_TARGET_DIR
        cc -O2 -static ci/r42/taskset-shim.c -o /usr/bin/taskset &&
        /usr/bin/taskset.real -c "$R42_CPU_FOUR" setpriv \
            --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all \
            --inh-caps=-all --ambient-caps=-all --no-new-privs \
            setsid --fork --wait bash -c '\''bash "$@"; exit $?'\'' bash \
                ci/r42/gauntlet.sh "$1" "$2" "$3" "$4"
        exit $?
    ' bash "$profile" "$head" "/lane/records/$name" "$plant" &
locker=$!
echo "$locker" > "$base/records/$name.locker.pid"
# Bash reaps its asynchronous command; the /proc entry disappears at exit.
# This loop never restarts a lock wait, and provides progress every minute.
while [[ -d "/proc/$locker" ]]; do
    echo "R42_JOB_ALIVE name=$name locker=$locker $(date -u +%FT%TZ)"
    sleep 60
done
wait "$locker"
rc=$?
echo "R42_JOB_EXIT name=$name exit=$rc $(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
du -sk "$base"
exit "$rc"
