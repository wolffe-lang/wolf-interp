#!/usr/bin/env python3
"""Release-metadata regression; run from any cwd against a committed tree.

Example: python3 ci/r42/release-posture.py --version 0.1.59 \
    --pin b16353e8ae616ff3a35bd27adc56b727c50df345
No runtime semantics or historical release entries are rewritten here.
"""
import argparse
import json
from pathlib import Path
import tomllib
import unittest

parser = argparse.ArgumentParser()
parser.add_argument("--version", required=True)
parser.add_argument("--pin", required=True)
args = parser.parse_args()
ROOT = Path(__file__).resolve().parents[2]


class ReleasePosture(unittest.TestCase):
    def test_manifest_and_lock_name_the_candidate(self):
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
        package = next(p for p in lock["package"] if p["name"] == "wolf-interp")
        self.assertEqual(manifest["package"]["version"], args.version)
        self.assertEqual(package["version"], args.version)

    def test_readme_version_transcript_names_the_candidate(self):
        self.assertIn(f"lupin {args.version}…", (ROOT / "README.md").read_text())

    def test_manual_protocol_transcript_names_the_candidate(self):
        text = (ROOT / "docs/manual/03-phases.md").read_text()
        self.assertIn(f'"impl_version":"{args.version}"', text)

    def test_changelog_names_pairing_and_unsupported_surface(self):
        text = (ROOT / "CHANGELOG.md").read_text()
        self.assertTrue(text.startswith(f"# Changelog\n\n## {args.version} — "))
        entry = text.split("\n## ", 2)[1]
        self.assertIn("wolf 0.2.37", entry)
        self.assertIn("unsupported", entry)
        for name in ("fs_statfs", "os_chroot", "os_getpriority",
                     "os_setpriority", "os_signal_action_one"):
            self.assertIn(f"`{name}`", entry)

    def test_existing_spec_and_anchor_pin_are_preserved(self):
        self.assertEqual((ROOT / "vendor/upstream/PIN").read_text().strip(), args.pin)
        expected = set((ROOT / "ci/is82/anchor-keys.txt").read_text().splitlines())
        actual = set(json.loads((ROOT / "vendor/upstream/spec/anchors.json").read_text())["anchors"])
        self.assertEqual(expected, actual)
        self.assertEqual(len(actual), 706)


unittest.main(argv=["release-posture"], verbosity=2)
