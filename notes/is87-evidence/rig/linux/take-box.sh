#!/usr/bin/env bash
# is87: one development take inside the toolchain container, on slot A's
# CPUs (take.sh holds the locks). Every step prints START and END lines and
# its output indented two spaces.
#   take-box.sh red | green | stress-red | stress-green
set -uo pipefail
export PATH=/opt/s263-rust/bin:$PATH
step() {
  local n=$1; shift
  local s=$SECONDS
  echo "START $n"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  echo "END $n exit=$rc secs=$((SECONDS - s))"
  return "$rc"
}
# The test binary cargo built for one test target.
bin_of() {
  cargo test --locked -j4 --test "$1" --no-run --message-format=json 2>/dev/null |
    python3 -I -c 'import json, sys
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == sys.argv[1] and "test" in m["target"]["kind"]:
        print(m["executable"])' "$1"
}
# RUNS runs of the argv_is84 binary, PAR at once (the load), each with the
# default test threads; every failing run's log is kept, and every failing
# test is counted by name.
stress() {
  local runs=$1 par=$2 bin
  bin=$(bin_of argv_is84)
  test -x "$bin" || { echo "no argv_is84 binary"; return 2; }
  echo "binary $bin; $runs runs, $par at once, on $(nproc) CPUs"
  local d=/lane/tmp/stress-$$
  rm -rf "$d"; mkdir -p "$d"
  seq 1 "$runs" | xargs -P "$par" -I{} bash -c '"$1" > "$2/run-{}.log" 2>&1; echo $? > "$2/run-{}.rc"' bash "$bin" "$d"
  local failed=0
  for rc in "$d"/run-*.rc; do [ "$(cat "$rc")" = 0 ] || failed=$((failed + 1)); done
  echo "STRESS runs=$runs failed_runs=$failed"
  grep -h '^test .* FAILED$' "$d"/run-*.log | sort | uniq -c | sed 's/^/FAILED-TEST /'
  grep -ho 'Text file busy' "$d"/run-*.log | sort | uniq -c | sed 's/^/ETXTBSY-LINES /'
  grep -h 'scripts were refused with ETXTBSY' "$d"/run-*.log | sed 's/(tries.*) while/while/' | sort | uniq -c | sed 's/^/ROW-REPORT /'
  for log in "$d"/run-*.log; do
    if [ "$(cat "${log%.log}.rc")" != 0 ]; then echo "--- first failing run: $log"; grep -A6 'panicked at' "$log" | head -30; break; fi
  done
  rm -rf "$d"
  [ "$failed" = 0 ]
}
step head bash -c 'git rev-parse HEAD; git log --oneline -1; rustc -V; cargo -V; nproc; uname -r; grep -E "^Sig(Blk|Ign):" /proc/self/status; env | grep ^CARGO_PROFILE | sort'
step build cargo test --locked -j4 --test pty_s292 --test argv_is84 --no-run || exit $?
case "$1" in
  red | green)
    step pty-rows cargo test --locked -j4 --test pty_s292 -- --test-threads=8
    step argv-rows cargo test --locked -j4 --test argv_is84 -- --test-threads=8
    if [ "$1" = green ]; then
      step fmt cargo fmt --all --check
      step clippy cargo clippy --locked -j4 --all-targets -- -D warnings
      step lib cargo test --locked -j4 --lib -- --test-threads=8
      step neighbours cargo test --locked -j4 --test session_s290 --test os_mirror_is83 --test os_mirror_target_is83 --test term_editor_s234 --test editor_calls_is79 --test job_s219 --test fs_family -- --test-threads=8
    fi
    ;;
  stress-red)
    step stress stress "${STRESS_RUNS:-40}" 4
    ;;
  stress-green)
    step stress stress "${STRESS_RUNS:-200}" 4
    ;;
esac
echo "TAKE-BOX done $(date -u +%FT%TZ)"
