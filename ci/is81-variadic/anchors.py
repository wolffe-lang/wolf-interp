#!/usr/bin/env python3
"""Hold the complete pinned anchor key sets to each other, in both directions.

The pinned set is the manifest `tests/variadic_is81.rs` includes, so the
lint job and the test cannot pin two different vendors (is82, run
41274704925: this read is81's 705 keys after the b16353e8 re-vendor).
"""
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
expected = set((ROOT / "ci/is82/anchor-keys.txt").read_text().splitlines())
actual = set(json.loads((ROOT / "vendor/upstream/spec/anchors.json").read_text())["anchors"])
lost, added = sorted(expected - actual), sorted(actual - expected)
print(f"pinned anchors: expected={len(expected)} actual={len(actual)}")
print(f"lost anchors: {lost}")
print(f"unexpected anchors: {added}")
if lost or added:
    print("::error::pinned_anchor_keys_agree_both_ways failed")
    sys.exit(1)
