#!/usr/bin/env bash
# li-wait: the build profile every step passes into its container, checked
# before the first one: CI's heavy-entry profile, opt-level 2 with debug
# assertions and overflow checks on.
set -u
rc=0
for var in CARGO_PROFILE_DEV_OPT_LEVEL CARGO_PROFILE_DEV_DEBUG_ASSERTIONS CARGO_PROFILE_DEV_OVERFLOW_CHECKS CARGO_PROFILE_TEST_OPT_LEVEL CARGO_PROFILE_TEST_DEBUG_ASSERTIONS CARGO_PROFILE_TEST_OVERFLOW_CHECKS; do
  value=$(printenv "$var") || value=unset
  printf '%s=%s\n' "$var" "$value"
  case "$var" in *_OPT_LEVEL) expected=2 ;; *) expected=true ;; esac
  test "$value" = "$expected" || rc=1
done
exit "$rc"
