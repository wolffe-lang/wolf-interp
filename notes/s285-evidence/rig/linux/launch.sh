#!/usr/bin/env bash
set -euo pipefail
lane=$LANE/interp
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
exec "${S285_REAL_GIT:?}" -c commit.gpgsign=false "$@"
GIT
chmod +x "$lane/tool-bin/git"
export S285_REAL_GIT="$real_git"
export PATH="$lane/tool-bin:$PATH"
# s285: a fresh clone of the pushed branch from GitHub (its origin the GitHub
# URL, so the record names the repository), never a bundle.
test ! -e "$lane/wolf-interp"
git -c commit.gpgsign=false clone --branch s285 https://github.com/wolffe-lang/wolf-interp.git "$lane/wolf-interp"
test "$(git -c commit.gpgsign=false -C "$lane/wolf-interp" rev-parse HEAD)" = "$sha"
cd "$lane/wolf-interp"
bash "$lane/gauntlet/record.sh" --output "$lane/green-$sha.log" -- bash "$lane/gauntlet/driver.sh"
