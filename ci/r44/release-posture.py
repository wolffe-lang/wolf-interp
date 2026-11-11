#!/usr/bin/env python3
"""Release-metadata regression for lupin 0.1.61; offline, any cwd.

Example: python3 ci/r44/release-posture.py --version 0.1.61 \
    --pin 5f1f1c949582f53121e4047bc83c86f23ff72240 --wolf 0.2.39

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
        # The session and pseudo-terminal mirrors: every name resolves,
        # the shapes are answered, the rest is refused by name.
        for name in ("os_spawn_session", "os_setsid", "os_sid", "os_term_open_pty",
                     "os_term_pty_name", "os_term_set_size", "os_term_attr",
                     "os_term_set_attr"):
            self.assertIn(f"`{name}", e)
        self.assertIn("refused by name", e)
        for row in ("corpus/os/session_rows.lu", "corpus/os/pty_rows.lu"):
            self.assertIn(row, e)
        # The front-end rulings, the undeclared name and the literal pattern,
        # each at the compiler's code.
        for code in ("E0301", "E0604", "E0410", "E0302", "E0827", "E0828", "E0404",
                     "E0601", "E0609", "E0207", "E0415"):
            self.assertIn(code, e)
        self.assertIn("`overflow`", e)
        # The test that raced a fork, and the corpus walk that fits again.
        for issue in ("#360", "#358"):
            self.assertIn(issue, e)

    def test_previous_entry_is_the_published_one(self):
        e = entry((ROOT / "CHANGELOG.md").read_text(), "0.1.60")
        self.assertIn("wolf 0.2.38", e)
        self.assertIn("W0801", e)

    def test_spec_and_anchor_pin_are_preserved(self):
        self.assertEqual((ROOT / "vendor/upstream/PIN").read_text().strip(), args.pin)
        expected = set((ROOT / "ci/is82/anchor-keys.txt").read_text().splitlines())
        actual = set(json.loads((ROOT / "vendor/upstream/spec/anchors.json").read_text())["anchors"])
        self.assertEqual(expected, actual)
        self.assertEqual(len(actual), 724)


unittest.main(argv=["release-posture"], verbosity=2)
