#!/usr/bin/env python3
"""Validate Cargo's actual native root-package artifact profiles, not env alone."""
import json
from pathlib import Path
import sys

rows = []
for line in Path(sys.argv[1]).read_text().splitlines():
    m = json.loads(line)
    if m.get('reason') != 'compiler-artifact':
        continue
    target = m['target']
    if not target.get('src_path', '').startswith('/work/'):
        continue
    if 'custom-build' in target.get('kind', []):
        continue  # build scripts use Cargo's distinct build-override profile
    profile = m['profile']
    assert profile['opt_level'] == '2', (target['name'], profile)
    assert profile['debug_assertions'] is True, (target['name'], profile)
    assert profile['overflow_checks'] is True, (target['name'], profile)
    rows.append((target['name'], tuple(target['kind']), profile['test'], m.get('executable')))
assert rows, 'no native root-package artifacts observed'
assert any('lib' in kind and test for _, kind, test, _ in rows), 'missing unit-test library artifact'
assert any('bin' in kind and not test and exe for _, kind, test, exe in rows), 'missing runtime executable'
names = {name for name, kind, test, exe in rows if 'test' in kind and test and exe}
required = {'doc_truth', 'run_corpus', 'export', 'os_mirror_is83', 'os_mirror_target_is83', 'nofollow_boundary_is83', 'binder_variant_s282'}
assert required <= names, ('missing required test executables', required - names)
print('PASS actual artifact profiles: opt_level=2 debug_assertions=true overflow_checks=true')
print(f'validated_root_artifacts={len(rows)} integration_test_executables={len(names)}')
for name, kind, test, exe in rows:
    print(f'  {name} kind={",".join(kind)} test={str(test).lower()} executable={exe or "none"}')
