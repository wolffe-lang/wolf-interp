#!/usr/bin/env python3
"""is79 (never merge): wolf-std e036bec's five lupin-windows cells sc68 wrote, under THIS branch's lupin, on this runner.

usage: is79-std-cells.py LUPIN WOLF_STD_DIR
Each row is staged as wolf-std's `xtask std-test` stages it (the file and a copy of std/ in a fresh directory) and
observed with `lupin conform-run FILE --json --std-root STAGE/std` from the stage. A row is `run` when the record's
verdict and stdout are the row's `//! check:` directive; anything else is printed as the record says it.
"""
import json, os, platform, re, shutil, subprocess, sys, tempfile

ROWS = ["signal/text_zero.lu", "signal/one_host.lu", "signal/one_fault.lu",
        "process/exit_signal_lives.lu", "process/wait_core_rows.lu"]

def declared(text):
    line = next(l for l in text.splitlines() if l.startswith("//! check:"))
    m = re.match(r'//! check: run\(exit=(\d+)(?:, stdout="(.*)")?\)\s*$', line)
    if not m:
        return None, line
    return f"exit({m.group(1)})", (m.group(2) or "").encode().decode("unicode_escape")

def main():
    lupin, std = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
    ver = subprocess.run([lupin, "--version"], capture_output=True, text=True).stdout.strip()
    print(f"is79 std cells: {platform.system()} {platform.machine()} lupin: {ver}")
    bad = 0
    for row in ROWS:
        src = os.path.join(std, "tests", row)
        text = open(src, encoding="utf-8").read()
        want_v, want_out = declared(text)
        stage = tempfile.mkdtemp(prefix="is79-")
        shutil.copy(src, os.path.join(stage, os.path.basename(row)))
        shutil.copytree(os.path.join(std, "std"), os.path.join(stage, "std"))
        out = subprocess.run([lupin, "conform-run", os.path.join(stage, os.path.basename(row)), "--json",
                              "--std-root", os.path.join(stage, "std")],
                             cwd=stage, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=600)
        rec = None
        for l in reversed(out.stdout.splitlines()):
            try:
                r = json.loads(l)
            except ValueError:
                continue
            if "verdict" in r:
                rec = r
                break
        if rec is None:
            print(f"  {row}: NO RECORD rc={out.returncode} stderr={out.stderr[-300:]!r}")
            bad += 1
            continue
        got_out = (rec.get("stdout_inline") or "").replace("\r", "").rstrip("\n")
        cell = "run" if (rec["verdict"] == want_v and got_out == (want_out or "").rstrip("\n")) else rec["verdict"]
        why = rec.get("x-unsupported") or ""
        print(f"  {row}: lupin cell `{cell}` (record {rec['verdict']}, stdout {got_out!r}; directive {want_v} {want_out!r}) {why[:160]}")
        shutil.rmtree(stage, ignore_errors=True)
    sys.exit(1 if bad else 0)

main()
