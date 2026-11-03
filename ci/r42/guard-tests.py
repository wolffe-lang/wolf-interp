#!/usr/bin/env python3
"""Small offline regression checks for r42's diagnostic and lifetime guards."""
import ast
import os
from pathlib import Path
import subprocess
import tempfile
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
        self.assertIn('exit "$rc"', runner)
        self.assertNotIn('--pid=host', runner)
        work = (ROOT / "ci/r42/container-work.sh").read_text()
        self.assertIn('--reuid=1000 --regid=1000', work)
        self.assertIn('setsid --fork --wait bash -c', work)
        self.assertIn('exit "$rc"', work)

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
        for script in (HELPER, ROOT / "ci/r42/hasu-run.sh", ROOT / "ci/r42/gauntlet.sh",
                       ROOT / "ci/r42/container-work.sh"):
            result = subprocess.run(["bash", "-n", str(script)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)


class OfflineWrapper(unittest.TestCase):
    def scratch(self):
        parent = ROOT / "target/r42-guard-tests"
        parent.mkdir(parents=True, exist_ok=True)
        return tempfile.TemporaryDirectory(dir=parent)

    def executable(self, path, text):
        path.write_text("#!/usr/bin/env bash\nset -eu\n" + text)
        path.chmod(0o755)

    def test_outer_inner_wrapper_preserves_order_and_nonzero_child_exit(self):
        with self.scratch() as directory:
            directory = Path(directory)
            trace = directory / "trace"
            self.executable(directory / "timeout", '''printf 'timeout %s\\n' "$*" >> "$TRACE"
test "$1" = --foreground
test "$2" = --kill-after=10
shift 3
exec "$@"
''')
            self.executable(directory / "flock", '''printf 'flock %s\\n' "$*" >> "$TRACE"
test "$1" = -w && test "$2" = 21600 && test "$3" = --close
shift 4
exec "$@"
''')
            self.executable(directory / "podman", '''printf 'podman %s\\n' "$*" >> "$TRACE"
test "$1" = run && test "$2" = --timeout
test "$3" -gt 0
exit 7
''')
            env = dict(os.environ, PATH=f"{directory}:{os.environ['PATH']}", TRACE=str(trace))
            result = subprocess.run(["bash", str(HELPER), "outer", "unused-inner.lock",
                                     str(directory / "podman"), "--rm"],
                                    env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 7, result.stderr)
            lines = trace.read_text().splitlines()
            self.assertEqual(len(lines), 4, lines)
            self.assertTrue(lines[0].startswith("timeout --foreground --kill-after=10 7000 flock "), lines)
            self.assertTrue(lines[1].startswith("flock -w 21600 --close unused-inner.lock bash "), lines)
            self.assertTrue(lines[2].startswith("timeout --foreground --kill-after=10 "), lines)
            self.assertTrue(lines[3].startswith("podman run --timeout "), lines)
            self.assertLessEqual(int(lines[2].split()[3]), 6970)
            self.assertLessEqual(int(lines[3].split()[3]), 6940)

    def test_real_plant_entrypoint_accepts_only_the_named_cargo_failure(self):
        with self.scratch() as directory:
            directory = Path(directory)
            plant = directory / "plant"
            plant.mkdir()
            (plant / "Cargo.toml").write_text('[package]\nname = "wolf-interp"\nversion = "0.1.59"\n')
            (plant / "Cargo.lock").write_text('[[package]]\nname = "wolf-interp"\nversion = "0.1.58"\n')
            self.executable(directory / "cargo", '''test "$*" = 'check --locked --offline --lib'
printf '%s\\n' "$DIAGNOSTIC" >&2
exit 101
''')
            env = dict(os.environ, PATH=f"{directory}:{os.environ['PATH']}")
            for diagnostic, expected in [
                ("error: cannot update the lock file /lane/Cargo.lock because --locked was passed to prevent this", 0),
                ("error: failed to download anstyle-wincon; offline was specified", 1),
            ]:
                with self.subTest(diagnostic=diagnostic):
                    env["DIAGNOSTIC"] = diagnostic
                    log = directory / "plant.log"
                    result = subprocess.run(["python3", "-I", str(ROOT / "ci/r42/plant-lock.py"),
                                             str(plant), str(log)], env=env,
                                            capture_output=True, text=True)
                    self.assertEqual(result.returncode, expected, result.stderr)
                    self.assertIn("END locked_manifest_refuses_stale_root_version exit=101", log.read_text())


if __name__ == "__main__":
    unittest.main(verbosity=2)
