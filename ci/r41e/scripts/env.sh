# r41e's container environment (sourced by every inner script).
ulimit -c 0
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export HOME=/lane/scratch/home TMPDIR=/lane/scratch/tmp XDG_CACHE_HOME=/lane/scratch/cache
export LUPIN_SIGNAL_TESTS=1
unset CARGO_TARGET_DIR
mkdir -p "$HOME" "$TMPDIR" "$XDG_CACHE_HOME"
