from pathlib import Path
import os
import subprocess

workflow = Path('/lane/src/.github/workflows/release.yml').read_text()

def block(name):
    part = workflow.split(f'      - name: {name}\n', 1)[1]
    part = part.split('        run: |\n', 1)[1]
    lines = []
    for line in part.splitlines():
        if line and not line.startswith('          '):
            break
        lines.append(line[10:])
    return '\n'.join(lines) + '\n'

os.chdir('/lane/tag-src')
target = Path('target')
if not target.exists():
    target.symlink_to('/lane/musl-target', target_is_directory=True)
env = dict(os.environ, TARGET='x86_64-unknown-linux-musl', RUNNER_OS='Linux',
           GITHUB_OUTPUT='/lane/evidence/musl-archive.outputs', RUNNER_TEMP='/lane/archive-smoke')
Path('/lane/archive-smoke').mkdir(exist_ok=True)
subprocess.run(['bash', '-c', block('build archive')], env=env, check=True)
outputs = dict(line.split('=', 1) for line in Path(env['GITHUB_OUTPUT']).read_text().splitlines())
smoke = block('unpack and smoke a hello')
for key, value in outputs.items():
    smoke = smoke.replace('${{ steps.archive.outputs.' + key + ' }}', value)
smoke = smoke.replace('${{ matrix.os }}', 'ubuntu-22.04')
subprocess.run(['bash', '-c', smoke], env=env, check=True)
print('exact archive and unpack/hello steps: green')
