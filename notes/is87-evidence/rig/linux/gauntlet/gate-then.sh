#!/usr/bin/env bash
# s289: hold a job, holding no lock, until FILE exists (another lane's
# priority job's exit marker), printing every 5 min; then exec the job.
#   gate-then.sh FILE CMD ARGS...
set -u
file=$1; shift
printf 'GATE_HOLD %s for %s (holding no lock)\n' "$(date -u +%FT%TZ)" "$file"
n=0
while ! test -e "$file"; do
  if [ $((n % 10)) -eq 0 ]; then printf 'GATE_HOLD %s\n' "$(date -u +%FT%TZ)"; fi
  n=$((n + 1)); sleep 30
done
printf 'GATE_GO %s %s exists\n' "$(date -u +%FT%TZ)" "$file"
exec "$@"
