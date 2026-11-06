#!/usr/bin/env python3
"""Release-metadata regression for lupin 0.1.60; offline, any cwd.

Example: python3 ci/r43/release-posture.py --version 0.1.60 \
    --pin 5f1f1c949582f53121e4047bc83c86f23ff72240 --wolf 0.2.38

The release's entry is found by its heading wherever it sits, so the
check still reads the published entry once a later `## Unreleased` opens
the changelog (wolf-interp#332's shape).
"""
import argparse
import json
from pathlib import Path
import re
import tomllib
import unittest

parser = argparse.ArgumentParser()
parser.add_argument("--version", required=True)
parser.add_argument("--pin", required=True)
parser.add_argument("--wolf", required=True)
args = parser.parse_args()
ROOT = Path(__file__).resolve().parents[2]


def entry(text, version):
    m = re.search(rf"^## {re.escape(version)} — \d{{4}}-\d{{2}}-\d{{2}}$", text, re.M)
    assert m, f"no `## {version} — <date>` heading"
    rest = text[m.end():]
    nxt = re.search(r"^## ", rest, re.M)
    return rest[: nxt.start()] if nxt else rest


class ReleasePosture(unittest.TestCase):
    def test_manifest_and_lock_name_the_release(self):
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
        package = next(p for p in lock["package"] if p["name"] == "wolf-interp")
        self.assertEqual(manifest["package"]["version"], args.version)
        self.assertEqual(package["version"], args.version)

    def test_readme_version_transcript_names_the_release(self):
        self.assertIn(f"lupin {args.version}…", (ROOT / "README.md").read_text())

    def test_manual_protocol_transcript_names_the_release(self):
        text = (ROOT / "docs/manual/03-phases.md").read_text()
        self.assertIn(f'"impl_version":"{args.version}"', text)

    def test_changelog_has_no_unreleased_section_at_the_release(self):
        text = (ROOT / "CHANGELOG.md").read_text()
        self.assertNotIn("## Unreleased", text)
        self.assertTrue(text.startswith(f"# Changelog\n\n## {args.version} — "))

    def test_changelog_entry_names_what_users_see(self):
        e = entry((ROOT / "CHANGELOG.md").read_text(), args.version)
        self.assertIn(f"wolf {args.wolf}", e)
        self.assertIn(args.pin[:8], e)
        # The argument boundary, and its migration, first.
        self.assertIn("**Breaking:**", e)
        self.assertIn("lupin FILE ARGS", e)
        # The OS mirror: what is served, what is declined by name.
        for name in ("fs_statfs", "os_chroot", "os_getpriority", "os_setpriority",
                     "os_signal_action_one", "fs_set_mode_fd", "fs_open_at", "RAW_LF"):
            self.assertIn(f"`{name}", e)
        self.assertIn("unsupported", e)
        for issue in ("#315", "#321"):
            self.assertIn(issue, e)
        # The s282 mirror, a strict subset of the compiler's W0801, with the
        # two compiler-only spans named.
        for word in ("W0801", "E0806", "lints/binder_capitalized.lu", "#344"):
            self.assertIn(word, e)

    def test_previous_entry_names_the_e0411_change(self):
        # wolf-interp#334: 5289391 shipped in 0.1.59 without a line.
        e = entry((ROOT / "CHANGELOG.md").read_text(), "0.1.59")
        self.assertIn("E0411", e)

    def test_spec_and_anchor_pin_are_preserved(self):
        self.assertEqual((ROOT / "vendor/upstream/PIN").read_text().strip(), args.pin)
        expected = set((ROOT / "ci/is82/anchor-keys.txt").read_text().splitlines())
        actual = set(json.loads((ROOT / "vendor/upstream/spec/anchors.json").read_text())["anchors"])
        self.assertEqual(expected, actual)
        self.assertEqual(len(actual), 724)


unittest.main(argv=["release-posture"], verbosity=2)
