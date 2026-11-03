#!/usr/bin/env python3
"""Prove the off-line stale-lock plant is red without weakening --locked."""
import argparse
from pathlib import Path
import subprocess
import tomllib


def is_locked_update_rejection(returncode, output):
    """Accept Cargo's two causal --locked diagnostics, never a generic failure."""
    if returncode == 0:
        return False
    for line in output.lower().splitlines():
        line = line.strip()
        if not line.startswith("error:") or "lock file" not in line:
            continue
        if "needs to be updated but --locked was passed to prevent this" in line:
            return True
        if (line.startswith("error: cannot update the lock file ")
                and "because --locked was passed to prevent this" in line):
            return True
    return False


def main():
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
    assert is_locked_update_rejection(result.returncode, output), (
        f"{row}: expected Cargo's stale-lock rejection, got exit={result.returncode}\n{output}"
    )
    print(f"Plant red by name: {row}; actual exit={result.returncode}; log={args.log}")


if __name__ == "__main__":
    main()
