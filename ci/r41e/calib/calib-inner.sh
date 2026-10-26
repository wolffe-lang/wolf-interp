#!/usr/bin/env bash
set -u
ulimit -c 0
sig() { grep -E '^(Name|SigBlk|SigIgn|SigCgt):'; }
echo "R41E_CALIB_START $(date -u +%FT%TZ)"
echo "== the container shell"; sig < /proc/self/status
for v in 0.1.56 0.1.57; do
  case $v in 0.1.56) key=lupin056;; 0.1.57) key=lupin057;; esac
  bin=/r41-inputs/$key/lupin-$v-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-$v-x86_64-unknown-linux-gnu/lupin
  "$bin" --version
  w=/lane/calib/work-$v; rm -rf "$w"; mkdir -p "$w"; cd "$w"
  echo "== $v exec, plain start"; "$bin" run /lane/calib/exec_cat.lu | sig; echo "rc=${PIPESTATUS[0]}"
  echo "== $v exec, start with PIPE+USR1 blocked"; python3 -I /lane/calib/launch.py "$bin" run /lane/calib/exec_cat.lu | sig; echo "rc=${PIPESTATUS[0]}"
  echo "== $v spawn paths, start with PIPE+USR1 blocked"; python3 -I /lane/calib/launch.py "$bin" run /lane/calib/spawn_paths.lu | grep -E '^(==|Name|SigBlk|SigIgn|SigCgt)'; echo "rc=${PIPESTATUS[0]}"
  printf 'echo INTERPRETER-TEXT-RAN-UNDER-SH\n' > interp; chmod 755 interp
  printf '#!%s/interp\necho SCRIPT-TEXT-RAN-UNDER-SH\nexit 71\n' "$w" > script; chmod 755 script
  echo "== $v os_exec of a #! script whose interpreter has no header"; "$bin" run /lane/calib/shebang_exec.lu; echo "rc=$?"
  echo "== $v os_spawn of the same script"; "$bin" run /lane/calib/shebang_spawn.lu; echo "rc=$?"
done
echo "== the kernel's own answer for the script (python execve, no fallback)"
python3 -I -c 'import os,sys
try:
    os.execv(sys.argv[1], [sys.argv[1]])
except OSError as e:
    print("execve errno", e.errno)' /lane/calib/work-0.1.57/script
echo "R41E_CALIB_END $(date -u +%FT%TZ)"
