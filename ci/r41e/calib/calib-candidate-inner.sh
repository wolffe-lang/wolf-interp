#!/usr/bin/env bash
# r41e: the calibration programs against the candidate lupin.
set -u
ulimit -c 0
sig() { grep -E '^(==|Name|SigBlk|SigIgn|SigCgt)'; }
echo "R41E_CALIB_CANDIDATE_START $(date -u +%FT%TZ)"
for bin in "$@"; do
  "$bin" --version; sha256sum "$bin"
  w=/lane/calib/work-candidate; rm -rf "$w"; mkdir -p "$w"; cd "$w"
  echo "== exec, plain start"; "$bin" run /lane/calib/exec_cat.lu | sig; echo "rc=${PIPESTATUS[0]}"
  echo "== exec, start with PIPE+USR1 blocked"; python3 -I /lane/calib/launch.py "$bin" run /lane/calib/exec_cat.lu | sig; echo "rc=${PIPESTATUS[0]}"
  echo "== spawn paths, start with PIPE+USR1 blocked"; python3 -I /lane/calib/launch.py "$bin" run /lane/calib/spawn_paths.lu | sig; echo "rc=${PIPESTATUS[0]}"
  echo "== lupin's own status around a failed exec, plain start"; "$bin" run /lane/calib/failed_status.lu | sig; echo "rc=${PIPESTATUS[0]}"
  printf 'echo INTERPRETER-TEXT-RAN-UNDER-SH\n' > interp; chmod 755 interp
  printf '#!%s/interp\necho SCRIPT-TEXT-RAN-UNDER-SH\nexit 71\n' "$w" > script; chmod 755 script
  echo "== os_exec of a #! script whose interpreter has no header"; "$bin" run /lane/calib/shebang_exec.lu; echo "rc=$?"
done
echo "R41E_CALIB_CANDIDATE_END $(date -u +%FT%TZ)"
