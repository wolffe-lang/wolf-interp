#!/usr/bin/env python3
"""Reconcile an interrupted/resumed cargo run against every test target.

Inputs, in execution order: cargo --nocapture logs. The last completed result
for each target wins; earlier failures remain in the supplied logs. No target
may be missing or fail. This is evidence tooling, not a replacement CI gate.
"""
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[2]
expected = {'lib', 'bins', 'doc'} | {p.stem for p in (root / 'tests').glob('*.rs')}
results = {}
skips = []
notices = []
for name in sys.argv[1:]:
    current = None
    for raw in Path(name).read_text().splitlines():
        line = re.sub(r'\x1b\[[0-9;]*m', '', raw)
        if 'Running unittests src/lib.rs ' in line:
            current = 'lib'
        elif 'Running unittests src/main.rs ' in line:
            current = 'bins'
        elif m := re.search(r'Running tests/([^/\s]+)\.rs ', line):
            current = m[1]
        elif 'Doc-tests wolf_interp' in line:
            current = 'doc'
        if m := re.match(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; .*?(\d+) filtered out;', line):
            assert current, f'result without target: {name}: {line}'
            results[current] = (m[1], int(m[2]), int(m[3]), int(m[4]), int(m[5]), Path(name).name)
        if line.startswith('SKIP'):
            skips.append((current, line))
        if 'submodule absent; vendored snapshot not cross-checked here' in line:
            notices.append(line)

missing = expected - results.keys()
extra = results.keys() - expected
assert not missing and not extra, f'target reconciliation: missing={sorted(missing)} extra={sorted(extra)}'
assert all(r[0] == 'ok' and r[2] == 0 and r[4] == 0 for r in results.values()), results
for target, result in sorted(results.items()):
    print(f'{target}\tpassed={result[1]}\tignored={result[3]}\t{result[5]}')
print(f'{len(expected)} of {len(expected)} test targets accounted for; '
      f'{sum(r[1] for r in results.values())} passed; '
      f'{sum(r[3] for r in results.values())} ignored; 0 failed; 0 filtered')
print(f'{len(skips)} loud SKIP line(s):')
for target, line in skips:
    print(f'  {target}: {line}')
for line in sorted(set(notices)):
    print(f'NOTICE: {line}')
