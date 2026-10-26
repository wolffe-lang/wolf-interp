# r41e's Mac environment: lane-local HOME/TMPDIR/CARGO_HOME, the pinned toolchain.
export HOME=/Users/mfwolffe/lanes/r41e/home
export TMPDIR=/Users/mfwolffe/lanes/r41e/tmp
export CARGO_HOME=/Users/mfwolffe/lanes/r41e/cargo-home
export RUSTUP_HOME=/Users/mfwolffe/.rustup
export RUSTUP_TOOLCHAIN=1.97.1
export PATH=/Users/mfwolffe/.rustup/toolchains/1.97.1-aarch64-apple-darwin/bin:/usr/bin:/bin:/usr/sbin:/sbin
export CARGO_TARGET_DIR=/Users/mfwolffe/lanes/r41e/wolf-interp/target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
# The 8 GiB floor: free + inactive + speculative, retried every 180 s.
r41e_gate() {
  while :; do
    local avail
    avail=$(vm_stat | awk '/page size of/ {ps=$8} /Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {gsub(/\./,"",f); gsub(/\./,"",i); gsub(/\./,"",s); printf "%d", (f+i+s)*ps/1048576}')
    if [ "$avail" -ge 8192 ]; then echo "R41E_GATE ok ${avail} MiB at $(date -u +%FT%TZ)"; return 0; fi
    echo "R41E_GATE wait ${avail} MiB at $(date -u +%FT%TZ)"; sleep 180
  done
}
