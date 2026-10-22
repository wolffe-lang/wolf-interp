#!/usr/bin/env python3
"""Hold the static archive's release wiring and its refusing ELF gate."""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class MuslDist(unittest.TestCase):
    def test_release_has_a_separate_musl_entry_and_runs_the_gate(self):
        workflow = (ROOT / '.github/workflows/release.yml').read_text()
        self.assertIn('target: x86_64-unknown-linux-musl', workflow)
        self.assertIn('ci/musl-build.sh', workflow)
        self.assertIn('ci/musl-check.sh', workflow)
        self.assertIn('target/${TARGET}/release/lupin', workflow)
        self.assertIn('matrix.target', workflow)
        self.assertIn('tags: ["v*"]', workflow)
        self.assertNotIn('workflow_dispatch', workflow)

    def test_elf_gate_refuses_pie_interpreter_and_dynamic_segments(self):
        gate = ROOT / 'ci/musl-check.sh'
        self.assertTrue(gate.is_file(), 'the release needs an executable ELF gate')
        with tempfile.TemporaryDirectory(prefix='is80-dist-') as d:
            d = Path(d)
            fake = d / 'readelf'
            fake.write_text('''#!/usr/bin/env bash
case "$1" in
-hW) echo "Type: ${ELF_TYPE:-EXEC}" ;;
-lW) echo "${ELF_SEGMENT:-LOAD}" ;;
-SW) echo "${ELF_SECTION:-.text}" ;;
-dW) echo "${ELF_DYNAMIC:-There is no dynamic section in this file.}" ;;
esac
''')
            fake.chmod(0o755)
            binary = d / 'lupin'
            binary.write_bytes(b'ELF gate test input\n')
            env = dict(os.environ, PATH=f'{d}:{os.environ["PATH"]}')
            for fields, succeeds in [
                ({}, True), ({'ELF_TYPE': 'DYN'}, False),
                ({'ELF_SEGMENT': 'INTERP'}, False),
                ({'ELF_SEGMENT': 'DYNAMIC'}, False),
                ({'ELF_SECTION': '.interp'}, False),
                ({'ELF_SECTION': '.dynamic'}, False),
                ({'ELF_DYNAMIC': 'Dynamic section at offset 0x100'}, False),
            ]:
                result = subprocess.run(['bash', str(gate), str(binary)],
                                        env=dict(env, **fields), capture_output=True, text=True)
                self.assertEqual(result.returncode == 0, succeeds,
                                 f'{fields}: {result.stdout} {result.stderr}')
                if succeeds:
                    self.assertIn('sha256', result.stdout)


if __name__ == '__main__':
    unittest.main()
