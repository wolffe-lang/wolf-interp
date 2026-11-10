#!/bin/bash
# is86: a log or script as committed: the lane's own paths written $LANE,
# other lanes' shared paths $LANES, the home directory $HOME, the Linux host
# H1. Reads stdin, writes stdout; fails if a private word survives.
set -euo pipefail
# PRIVATE: the host, user and repository names the lane must never publish
# (the lane's copy spells them; this copy does not).
PRIVATE=${PRIVATE:?}
out=$(sed -E \
  -e 's#/Users/USER/lanes[/]is86#$LANE#g' \
  -e 's#/home/USER/lanes[/]is86#$LANE#g' \
  -e 's#/Users/USER/lanes#$LANES#g' \
  -e 's#/home/USER/lanes#$LANES#g' \
  -e 's#/Users/USER#$HOME#g' \
  -e 's#/home/USER#$HOME#g' \
  -e 's#~/lanes#$LANES#g' \
  -e 's#[$]HOME/lanes#$LANES#g' \
  -e 's#H1-HOST#H1#g' \
  -e 's#USER-NAME#USER#g')
if printf '%s\n' "$out" | grep -qiE "$PRIVATE"; then
  printf '%s\n' "$out" | grep -niE "$PRIVATE" >&2
  echo "sanitize: a private word survives" >&2
  exit 1
fi
printf '%s\n' "$out"
