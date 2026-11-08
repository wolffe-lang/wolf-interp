#!/usr/bin/env bash
# s288 on H2: one check take at a commit, detached by the caller (setsid
# -f, pid, log, exit marker). The commit arrives as a git bundle; a worktree is
# made at that sha BEFORE any lock is waited on (build from a commit). Crates
# are fetched outside the heavy locks, as a light job. Then, under both locks
# (A outside, then B) and the wineserver and heat gate, the toolchain image
# runs mac-steps.sh on slot A's CPUs. TERM, INT and HUP write their own codes.
#   take.sh NAME SHA BUNDLE [STEPS...]
set -u
lane=$LANE/interp
name=$1; sha=$2; bundle=$3; shift 3
mark=$lane/iter-$name-${sha:0:8}.exit
on_signal() { echo "$1" > "$mark"; echo "SIGNAL $1 at $(date -u +%FT%TZ)"; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
g() { git -c commit.gpgsign=false "$@"; }
mkdir -p "$lane/tmp" "$lane/cargo-home"
if [ ! -d "$lane/base" ]; then g clone -q https://github.com/wolffe-lang/wolf-interp.git "$lane/base" || { echo 1 > "$mark"; exit 1; }; fi
g -C "$lane/base" fetch -q "$bundle" "+refs/heads/s288:refs/remotes/bundle/s288" || { echo 1 > "$mark"; exit 1; }
w=$lane/iter-${sha:0:8}
if [ ! -d "$w" ]; then g -C "$lane/base" worktree add -q --detach "$w" "$sha" || { echo 1 > "$mark"; exit 1; }; fi
test "$(g -C "$w" rev-parse HEAD)" = "$sha" || { echo 1 > "$mark"; exit 1; }
echo "TAKE $name at $sha in $w $(date -u +%FT%TZ)"
$LANES/H2-env/bin/light bash -c '
  exec $LANES/H2-env/bin/podman run --rm --memory "$LIGHT_MEM" \
    -v $LANES:$LANES -v "$1:/lane" -v "$2:/work" -w /work \
    -e CARGO_HOME=/lane/cargo-home -e LIGHT_CPUS \
    localhost/r41-toolchain:1 bash -c "export PATH=/opt/s263-rust/bin:\$PATH; /usr/bin/taskset.real -c \"\$LIGHT_CPUS\" cargo fetch --locked"
' bash "$lane" "$w" || { echo 1 > "$mark"; exit 1; }
while [ -n "$(ps -C wineserver -o pid= | tr -d ' ')" ]; do echo "wineserver running $(date -u +%FT%TZ); waiting"; sleep 300; done
flock -w 21600 $LANES/H2-A.lock flock -w 21600 $LANES/H2-B.lock \
  bash "$lane/gauntlet/gate.sh" \
  $LANES/H2-env/bin/podman run --rm --timeout 6600 --memory 12g --pids-limit 16384 \
    --name "s288-iter-$name-${sha:0:8}" --label lane=s288 \
    -v $LANES:$LANES -v "$lane:/lane" -v "$w:/work" -w /work \
    -e TMPDIR=/lane/tmp -e CARGO_HOME=/lane/cargo-home -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=8 -e CARGO_NET_OFFLINE=true \
    localhost/r41-toolchain:1 bash -c 'export PATH=/opt/s263-rust/bin:$PATH; /usr/bin/taskset.real -c 0-3,8-11 bash /lane/iter/steps.sh "$@"' bash "$@" &
child=$!
wait "$child"; rc=$?
echo "$rc" > "$mark"
echo "TAKE-END $name rc=$rc $(date -u +%FT%TZ)"
exit "$rc"
