#!/usr/bin/env bash
# s288: judge every embedded program (sweep.py) with the base and the head
# lupin at the resolve rung, inside the container; print each program whose
# verdict or diagnostics moved, with where it is embedded.
#   sweep.sh BASE_LUPIN HEAD_LUPIN OUTDIR
set -u
base=$(realpath "$1"); head=$(realpath "$2"); out=$3
python3 -I /lane/iter/sweep.py "$out" /work
moved=0; total=0
while IFS=$'\t' read -r id where; do
  total=$((total + 1))
  a=$(cd "$out/$id" && timeout 20 "$base" conform-run --json --phase=resolve main.lu 2>/dev/null | python3 -I -c 'import json,sys
try:
    d=json.load(sys.stdin); print(d["verdict"], [(x["code"],x["span"]) for x in d["diagnostics"] if x.get("severity")!="warning"])
except Exception as e: print("ERR")')
  b=$(cd "$out/$id" && timeout 20 "$head" conform-run --json --phase=resolve main.lu 2>/dev/null | python3 -I -c 'import json,sys
try:
    d=json.load(sys.stdin); print(d["verdict"], [(x["code"],x["span"]) for x in d["diagnostics"] if x.get("severity")!="warning"])
except Exception as e: print("ERR")')
  if [ "$a" != "$b" ]; then
    moved=$((moved + 1))
    printf 'MOVED %s %s\n  base: %s\n  head: %s\n' "$id" "$where" "$a" "$b"
  fi
done < "$out/index.tsv"
printf 'sweep: %s programs, %s moved\n' "$total" "$moved"
