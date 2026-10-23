"""Black-box probes; all executable children belong to this container run.

Usage: measure.py REPO WORK OUT WOLF OLD_LUPIN NEW_LUPIN STD_ROOT
The sc70 file is test data from wolf-std 2386522e, not implementation.
"""
from pathlib import Path
import json
import subprocess
import sys

repo, work, out, wolf, old, new, std = map(Path, sys.argv[1:])
work.mkdir(parents=True, exist_ok=True)
out.mkdir(parents=True, exist_ok=True)
programs = {
    "sc70_local_format_rows": repo / "docs/is81-evidence/probes/local_format_rows.lu",
    "copy_chunk_failed_write": repo / "tests/fixtures/is81/copy_chunk_failed_write.lu",
    "copy_chunk_dev_full": repo / "tests/fixtures/is81/copy_chunk_dev_full.lu",
    "spawn_rows": repo / "tests/fixtures/is81/spawn_rows.lu",
    "exec_row": repo / "tests/fixtures/is81/exec_row.lu",
}
rows = []
for name, entry in programs.items():
    for label, binary, flags in [
        ("wolf-checked", wolf, ["--checked"]),
        ("wolf-native", wolf, ["--native"]),
        ("lupin-before", old, []),
        ("lupin-after", new, []),
    ]:
        cwd = work / f"{name}-{label}"
        cwd.mkdir(parents=True, exist_ok=True)
        (cwd / "target").mkdir(exist_ok=True)
        for file, data in [
            ("noshebang", b"echo ran > ran-marker\n"),
            ("junk", b"\0\0\0\0"),
            ("control", b"#!/bin/sh\nexit 7\n"),
        ]:
            p = cwd / file
            p.write_bytes(data)
            p.chmod(0o755)
        cmd = [str(binary), "conform-run", str(entry), "--json", *flags, "--std-root", str(std)]
        result = subprocess.run(cmd, cwd=cwd, capture_output=True, timeout=120, check=False)
        stem = out / f"{name}-{label}"
        stem.with_suffix(".stdout").write_bytes(result.stdout)
        stem.with_suffix(".stderr").write_bytes(result.stderr)
        try:
            record = json.loads(result.stdout)
        except (json.JSONDecodeError, UnicodeDecodeError):
            record = None
        row = {
            "probe": name,
            "tool": label,
            "command": cmd,
            "tool_exit": result.returncode,
            "marker": (cwd / "ran-marker").exists(),
            "record": record,
            "raw_stdout": result.stdout.decode("utf-8", "replace") if record is None else None,
        }
        rows.append(row)
        print(name, label, record.get("verdict") if record else "NO_RECORD", "marker=" + str(row["marker"]), flush=True)
(out / "measurements.json").write_text(json.dumps(rows, indent=2, sort_keys=True) + "\n")
