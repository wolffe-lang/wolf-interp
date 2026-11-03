#!/usr/bin/env python3
"""Check an off-tag candidate's actual CLI and protocol identity on Linux."""
import argparse
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("binary", type=Path)
parser.add_argument("--head", required=True)
parser.add_argument("--version", required=True)
parser.add_argument("--pin", required=True)
args = parser.parse_args()
binary = args.binary.resolve()
root = Path(__file__).resolve().parents[2]
short = subprocess.check_output(
    ["git", "rev-parse", "--short=7", args.head], cwd=root, text=True
).strip()
version = subprocess.check_output([binary, "--version"], text=True).strip()
print(version)
assert version.startswith(f"lupin {args.version}+dev.{short} "), version
assert args.pin[:7] in version, version
result = subprocess.run(
    [binary, "conform-run", "examples/squares.lu", "--json"],
    cwd=root, capture_output=True, text=True, check=True,
)
record = json.loads(result.stdout)
assert record["impl_version"] == args.version, record
assert args.head.startswith(record["commit"]), record
assert record["verdict"] == "exit(0)", record
assert record["stdout_inline"] == "sum of squares: 30\n", record
print("Candidate CLI/protocol identity matches the immutable commit and preserved pin")
