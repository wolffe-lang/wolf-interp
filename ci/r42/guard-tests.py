#!/usr/bin/env python3
"""Small offline regression checks for r42's diagnostic and lifetime guards."""
import ast
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "ci/r42/lock-budget.sh"


def rejection_predicate():
    source = ROOT / "ci/r42/plant-lock.py"
    tree = ast.parse(source.read_text())
    definitions = [node for node in tree.body
                   if isinstance(node, ast.FunctionDef)
                   and node.name == "is_locked_update_rejection"]
    if len(definitions) != 1:
        raise AssertionError("plant proof needs one semantic locked-update predicate")
    namespace = {}
    exec(compile(ast.Module(body=definitions, type_ignores=[]), str(source), "exec"), namespace)
    return namespace["is_locked_update_rejection"]


class PlantDiagnostic(unittest.TestCase):
    def test_old_cargo_locked_update_is_accepted(self):
        diagnostic = ("error: the lock file /lane/Cargo.lock needs to be updated "
                      "but --locked was passed to prevent this\n")
        self.assertTrue(rejection_predicate()(101, diagnostic))

    def test_cargo_197_locked_update_is_accepted(self):
        diagnostic = ("error: cannot update the lock file /lane/Cargo.lock "
                      "because --locked was passed to prevent this\n")
        self.assertTrue(rejection_predicate()(101, diagnostic))

    def test_success_never_counts_as_a_named_plant_red(self):
        diagnostic = ("error: cannot update the lock file /lane/Cargo.lock "
                      "because --locked was passed to prevent this\n")
        self.assertFalse(rejection_predicate()(0, diagnostic))

    def test_unrelated_failures_and_help_text_are_rejected(self):
        diagnostics = [
            "error: failed to download anstyle-wincon\nCaused by: offline was specified",
            "error: cannot update the lock file /lane/Cargo.lock: Permission denied (--locked)",
            "error: could not compile wolf-interp\nhelp: the lock file needs to be updated but --locked was passed",
            "help: cannot update the lock file /lane/Cargo.lock because --locked was passed to prevent this",
            "error: the lock file is unreadable; use --locked",
        ]
        predicate = rejection_predicate()
        for diagnostic in diagnostics:
            with self.subTest(diagnostic=diagnostic):
                self.assertFalse(predicate(101, diagnostic))


class LifetimeGuards(unittest.TestCase):
    def helper(self):
        self.assertTrue(HELPER.is_file(), "outer/inner lock budget helper is required")
        return HELPER.read_text()

    def test_outer_deadline_encloses_the_single_inner_wait(self):
        helper = self.helper()
        self.assertRegex(helper, r'exec timeout --foreground --kill-after=10 7000 \\\n\s+flock -w 21600 --close')
        self.assertLess(helper.index('export R42_LOCK_DEADLINE='), helper.index('exec timeout'))
        self.assertLessEqual(7000 + 10, 7200)
        runner = (ROOT / "ci/r42/hasu-run.sh").read_text()
        self.assertIn('flock -w 21600 --close "/home/mfwolffe/lanes/hasu-block-$block.lock"', runner)
        self.assertIn('ci/r42/lock-budget.sh" outer', runner)

    def test_cli_and_native_container_lifetimes_are_bounded(self):
        helper = self.helper()
        self.assertIn('timeout --foreground --kill-after=10 "$cli_budget"', helper)
        self.assertIn('"$podman" run --timeout "$container_budget"', helper)
        runner = (ROOT / "ci/r42/hasu-run.sh").read_text()
        self.assertIn('-e R42_LOCK_DEADLINE', runner)
        self.assertIn('ci/r42/lock-budget.sh work', runner)
        self.assertIn('timeout --foreground --kill-after=5 "$work_budget"', runner)
        self.assertIn('exit $?', runner)
        self.assertNotIn('--pid=host', runner)

    def test_remaining_budget_shrinks_after_inner_wait_and_startup(self):
        self.helper()
        for now, deadline, expected in [
                (100, 7100, "6970 6940 6910"),
                (6900, 7100, "170 140 110"),
                (6979, 7100, "91 61 31")]:
            with self.subTest(now=now):
                result = subprocess.run(["bash", str(HELPER), "plan", str(now), str(deadline)],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), expected)
        result = subprocess.run(["bash", str(HELPER), "work", "6900", "7100"],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "110")

    def test_insufficient_budget_refuses_startup(self):
        self.helper()
        for mode in ("plan", "work"):
            for now in (6980, 7100, 7200):
                with self.subTest(mode=mode, now=now):
                    result = subprocess.run(["bash", str(HELPER), mode, str(now), "7100"],
                                            capture_output=True, text=True)
                    self.assertEqual(result.returncode, 124, result.stderr)
                    self.assertEqual(result.stdout, "")

    def test_runner_and_budget_helper_parse_without_running_a_job(self):
        self.helper()
        for script in (HELPER, ROOT / "ci/r42/hasu-run.sh", ROOT / "ci/r42/gauntlet.sh"):
            result = subprocess.run(["bash", "-n", str(script)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
