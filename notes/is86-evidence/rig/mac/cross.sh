#!/bin/bash
# is86: the wasm library's clippy (CI's leg) and a windows-target clippy of
# every target, behind the lock. Output indented; END lines per step.
set -u
step() {
  local n=$1; shift
  echo "== STEP $n $(date -u +%T)"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  echo "END $n exit=$rc"
  return $rc
}
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l'
step wasm-clippy cargo clippy --locked --target wasm32-unknown-unknown --lib -- -D warnings
step windows-clippy cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings
echo "== done $(date -u +%FT%TZ)"
