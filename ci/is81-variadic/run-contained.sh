#!/bin/bash
set -euo pipefail
root=/home/mfwolffe/lanes/is81
exec flock /home/mfwolffe/lanes/kasumi-cores-0-3.lock \
 podman run --rm --init --userns=keep-id --user 0:0 --pids-limit 16384 --ulimit nproc=32768:32768 --memory=8g --cpus=4 \
 -v "$root/wolf-interp:/work" -v "$root/cargo-home:/cargo" -v "$root:/lane" \
 -v /home/mfwolffe/.rustup/toolchains/1.97.1-x86_64-unknown-linux-gnu:/opt/rust/rustup/toolchains/1.97.1-x86_64-unknown-linux-gnu:ro \
 -e HOME=/lane/home -e XDG_CONFIG_HOME=/lane/home/.config \
 -e WOLF_INTERP_STAMP_NONCE=is81-final-focused -e CARGO_BUILD_JOBS=4 -e CARGO_HOME=/cargo -e CARGO_INCREMENTAL=0 -e RUSTUP_AUTO_INSTALL=0 -e CARGO_TARGET_DIR=/work/target-contained -e TMPDIR=/work/.lane-tmp \
 -w /work --entrypoint /usr/bin/setpriv localhost/px57-rust:latest \
 --reuid "$(id -u)" --regid "$(id -g)" --clear-groups taskset -c 0-3 "$@"
