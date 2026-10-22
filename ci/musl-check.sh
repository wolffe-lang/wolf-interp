#!/usr/bin/env bash
# Refuse every shape PAX's static ELF loader cannot load. Inspect the binary
# that will be archived, not a guessed target default; print its digest.
set -euo pipefail
bin=${1:?usage: musl-check.sh BINARY}
header=$(readelf -hW "$bin")
programs=$(readelf -lW "$bin")
sections=$(readelf -SW "$bin")
dynamic=$(readelf -dW "$bin")
printf '%s\n%s\n%s\n%s\n' "$header" "$programs" "$sections" "$dynamic"
grep -Eq 'Type: +EXEC' <<< "$header"
if grep -Eq 'INTERP|DYNAMIC' <<< "$programs"; then
    echo 'static lupin must have neither PT_INTERP nor PT_DYNAMIC' >&2
    exit 1
fi
if grep -Eq '\.(interp|dynamic)([[:space:]]|$)' <<< "$sections"; then
    echo 'static lupin must have neither .interp nor .dynamic' >&2
    exit 1
fi
grep -q 'no dynamic section' <<< "$dynamic"
printf 'sha256 '
sha256sum "$bin"
