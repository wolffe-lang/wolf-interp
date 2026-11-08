#!/usr/bin/env python3
"""Exercise the actual profile verifier's red paths without compiling anything."""
from pathlib import Path
import copy
import json
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent
base = dict(reason='compiler-artifact', profile=dict(opt_level='2', debug_assertions=True, overflow_checks=True, test=True))
rows = []
for name, kind, test in [('wolf_interp', 'lib', True), ('lupin', 'bin', False)] + [(n, 'test', True) for n in ['doc_truth', 'run_corpus', 'export', 'os_mirror_is83', 'os_mirror_target_is83', 'nofollow_boundary_is83']]:
    m = copy.deepcopy(base)
    m['target'] = dict(name=name, kind=[kind], src_path='/work/src/lib.rs')
    m['profile']['test'] = test
    m['executable'] = '/work/target/debug/' + name
    rows.append(m)
with tempfile.TemporaryDirectory(prefix='profile-verifier-', dir=str(root)) as d:
    path = Path(d) / 'messages.jsonl'
    def run(data):
        path.write_text(''.join(json.dumps(m) + '\n' for m in data))
        return subprocess.run([sys.executable, '-I', str(root / 'verify-profile.py'), str(path)], capture_output=True, text=True)
    good = run(rows)
    assert good.returncode == 0, good.stderr
    print('PASS valid optimized-debug profile accepted')
    for field, wrong in [('opt_level', '0'), ('debug_assertions', False), ('overflow_checks', False)]:
        bad = copy.deepcopy(rows)
        bad[0]['profile'][field] = wrong
        result = run(bad)
        assert result.returncode != 0, field
        print('PASS verifier rejected planted ' + field + '=' + str(wrong))
