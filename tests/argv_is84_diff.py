#!/usr/bin/env python3
"""Black-box wolf run/lupin differential. Only their documented CLIs meet here."""
import argparse
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--wolf", required=True)
parser.add_argument("--lupin", required=True)
parser.add_argument("--scratch", required=True)
a = parser.parse_args()
root = Path(a.scratch).resolve()
root.mkdir(parents=True, exist_ok=True)
source = root / "args.lu"
source.write_text('//! member: false\nfn main() { for arg in env_args() { print("[{arg}]") } }\n')

def invoke(tool, argv):
    cmd = [tool, "run", str(source), *argv] if tool == a.wolf else [tool, str(source), *argv]
    out = subprocess.run(cmd, cwd=root, capture_output=True, timeout=90)
    assert out.returncode == 0, (cmd, out.returncode, out.stdout, out.stderr)
    return out.stdout

for name, argv, expected in [
    ("trailing-words", ["a", "b"], b"[a]\n[b]\n"),
    ("empty-unicode", ["", "é", "two words"], "[]\n[é]\n[two words]\n".encode()),
]:
    wolf, lupin = invoke(a.wolf, argv), invoke(a.lupin, argv)
    assert wolf == lupin == expected, (name, wolf, lupin, expected)
    print(f"PASS differential-{name}: byte-identical stdout")
source.write_text('//! member: false\nfn main() { print(env_arg0()) }\n')
wolf, lupin = invoke(a.wolf, []), invoke(a.lupin, [])
assert lupin == (str(source) + "\n").encode(), ("lupin argv0", lupin)
print(f"R-is84-1 wolf argv[0]={wolf.decode().strip()!r}; lupin argv[0]={lupin.decode().strip()!r}")
print("PASS differential-argv0: source spelling retained; compiler invocation convention recorded")
