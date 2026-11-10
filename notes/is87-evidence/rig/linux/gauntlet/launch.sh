#!/usr/bin/env bash
# is87: adapted by copy from s289's rig: the lane path, the IS87_ names, the container and label names, the branch, and the plant's test target.
set -euo pipefail
lane=$LANE
sha=$1
export TMPDIR="$lane/tmp"
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=true
export CARGO_PROFILE_DEV_OVERFLOW_CHECKS=true
export CARGO_PROFILE_TEST_OPT_LEVEL=2
export CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true
export CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true
real_git=$(command -v git)
mkdir -p "$lane/tmp" "$lane/tool-bin" "$lane/cargo-home"
cat > "$lane/tool-bin/git" <<'GIT'
#!/usr/bin/env bash
exec "${IS87_REAL_GIT:?}" -c commit.gpgsign=false "$@"
GIT
chmod +x "$lane/tool-bin/git"
export IS87_REAL_GIT="$real_git"
export PATH="$lane/tool-bin:$PATH"
# s289, via s288 (adapted by copy from s285): a fresh clone of the pushed branch from GitHub (its origin the GitHub
# URL, so the record names the repository), never a bundle.
test ! -e "$lane/wolf-interp"
git -c commit.gpgsign=false clone --branch "${IS87_BRANCH:-is87}" https://github.com/wolffe-lang/wolf-interp.git "$lane/wolf-interp"
test "$(git -c commit.gpgsign=false -C "$lane/wolf-interp" rev-parse HEAD)" = "$sha"
cd "$lane/wolf-interp"
# Every crate the build reads, fetched once here, outside the locks, by the
# checksums Cargo.lock pins; the record's steps then download nothing.
"$lane/gauntlet/prefetch.sh"
bash "$lane/gauntlet/record.sh" --output "$lane/green-$sha.log" -- bash "$lane/gauntlet/driver.sh"
