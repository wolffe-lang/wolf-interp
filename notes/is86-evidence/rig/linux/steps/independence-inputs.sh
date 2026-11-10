#!/usr/bin/env bash
set -euo pipefail
if grep -rEn 'upstream/(crates|xtask|src)' \
     --include='*.rs' --include='*.toml' --include='*.yml' \
     --exclude-dir=target --exclude-dir=upstream --exclude-dir=.git . ; then
  echo "::error::Something reaches into upstream/ outside spec/ and corpus/."
  exit 1
fi
