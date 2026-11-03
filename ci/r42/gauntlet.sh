#!/usr/bin/env bash
# Reproduce one exact-head half of r42's Linux gauntlet.
# Call from a committed snapshot, inside the bounded throwaway container.
# Args: debug|release, expected full head, records directory, optional plant tree.
set -uo pipefail
profile=${1:?debug or release}
expected=${2:?full head}
records=${3:?records directory}
plant=${4:-}
root=$(git rev-parse --show-toplevel) || exit $?
cd "$root" || exit $?
mkdir -p "$records"
step() {
    local name=$1 rc
    shift
    echo "START $name $(date -u +%FT%TZ)"
    "$@"
    rc=$?
    echo "END $name exit=$rc"
    if (( rc != 0 )); then exit "$rc"; fi
}
assert_head() {
    test "$(git rev-parse HEAD)" = "$expected" &&
        test -z "$(git status --porcelain --untracked-files=no)"
}
export WOLF_INTERP_STAMP_NONCE="$expected"
echo "R42_GAUNTLET profile=$profile head=$(git rev-parse HEAD) $(date -u +%FT%TZ)"
head -3 /etc/os-release
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|SigCgt' /proc/self/status
rustc --version
cargo --version
step exact-head assert_head
step release-posture python3 -I ci/r42/release-posture.py --version 0.1.59 \
    --pin b16353e8ae616ff3a35bd27adc56b727c50df345
step fmt cargo fmt --check
step tiers-test python3 -I ci/tiers-test.py
step dist-test python3 -I ci/is80/dist-test.py
step anchors python3 -I ci/is81-variadic/anchors.py
case "$profile" in
    debug)
        step clippy cargo clippy --locked --all-targets -- -D warnings
        step locked-metadata bash -c 'cargo metadata --locked --offline --format-version=1 > "$1"' \
            bash "$records/locked-metadata.json"
        if [[ -n "$plant" ]]; then
            step plant-lock-proof python3 -I ci/r42/plant-lock.py "$plant" "$records/plant-lock.log"
        fi
        step debug-tests cargo test --locked \
            --test differ_cli --test build_stamp --test mirror_is82 \
            --test exec_sigpipe_r41e --test cli --test variadic_is81 \
            --test anchor_admission -- --nocapture
        step debug-binary-posture python3 -I ci/r42/binary-posture.py target/debug/lupin \
            --head "$expected" --version 0.1.59 \
            --pin b16353e8ae616ff3a35bd27adc56b727c50df345
        ;;
    release)
        step build-release cargo build --release --locked
        step release-binary-posture python3 -I ci/r42/binary-posture.py target/release/lupin \
            --head "$expected" --version 0.1.59 \
            --pin b16353e8ae616ff3a35bd27adc56b727c50df345
        step binary-digest-first sha256sum target/release/lupin
        step binary-digest-second sha256sum target/release/lupin
        # The two rows assert an UNOPTIMIZED harness. They run unfiltered
        # in debug-tests above; release does not pretend to be debug.
        echo 'Release excludes exactly two debug-only differ_cli rows; both run in debug-tests'
        step release-tests cargo test --release --locked -- --nocapture \
            --skip a_debug_harness_is_refused_by_name_before_it_can_compare \
            --skip the_door_is_loud_and_stamps_the_profile_on_the_report
        step corpus bash -c 'target/release/lupin corpus --json > "$1"' \
            bash "$records/corpus-candidate.json"
        step place-trace-identity bash ci/place-trace-identity.sh target/release/lupin
        ;;
    *) echo "unknown profile: $profile" >&2; exit 2 ;;
esac
step fmt-again cargo fmt --check
step clean-tree assert_head
echo "R42_GAUNTLET_GREEN profile=$profile head=$expected $(date -u +%FT%TZ)"
