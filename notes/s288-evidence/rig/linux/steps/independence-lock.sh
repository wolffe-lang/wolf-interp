#!/usr/bin/env bash
set -euo pipefail
cp Cargo.lock /lane/lock-original
trap 'cp /lane/lock-original Cargo.lock' EXIT
cargo generate-lockfile --offline
if grep -nE '^name = "wolf[-_]' Cargo.lock | grep -v '"wolf-interp"'; then exit 1; fi
if grep -n 'wolf-lang' Cargo.lock; then exit 1; fi
