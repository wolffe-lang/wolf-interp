#!/usr/bin/env python3
"""Prove the off-line stale-lock plant is red without weakening --locked."""
import argparse
from pathlib import Path
import subprocess
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument("plant", type=Path)
parser.add_argument("log", type=Path)
args = parser.parse_args()
manifest = tomllib.loads((args.plant / "Cargo.toml").read_text())
lock = tomllib.loads((args.plant / "Cargo.lock").read_text())
package = next(p for p in lock["package"] if p["name"] == "wolf-interp")
assert manifest["package"]["version"] == "0.1.59"
assert package["version"] == "0.1.58"
result = subprocess.run(
    ["cargo", "check", "--locked", "--offline", "--lib"],
    cwd=args.plant, capture_output=True, text=True,
)
output = result.stdout + result.stderr
row = "locked_manifest_refuses_stale_root_version"
args.log.parent.mkdir(parents=True, exist_ok=True)
args.log.write_text(
    f"PLANT {row}\n{output}\nEND {row} exit={result.returncode}\n"
)
assert result.returncode != 0, f"{row}: the planted stale lock was accepted"
assert "lock file" in output.lower() and "--locked" in output, output
assert "needs to be updated" in output.lower(), output
print(f"Plant red by name: {row}; actual exit={result.returncode}; log={args.log}")
