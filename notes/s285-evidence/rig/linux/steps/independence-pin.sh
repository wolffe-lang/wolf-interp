#!/usr/bin/env bash
set -euo pipefail
git submodule status --cached upstream
test -n "$(git rev-parse HEAD:upstream)"

