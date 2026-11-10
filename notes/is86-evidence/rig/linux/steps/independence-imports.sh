#!/usr/bin/env bash
set -euo pipefail
if grep -rn '^\s*(pub )?use +wolf_(?!interp)|^\s*extern crate +wolf_' \
     -P --include='*.rs' \
     --exclude-dir=target --exclude-dir=upstream --exclude-dir=.git . ; then
  echo "::error::A source file imports a wolf-lang crate; the frontend is written from spec/ only."
  exit 1
fi
