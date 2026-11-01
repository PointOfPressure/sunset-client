"""Compile the production rival store and exercise real isolated filesystem fixtures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    inputs = ['mixins/sunset/rival_store.rs', 'tools/rival_store_tests.rs']
    hashes = {name: hashlib.sha256((repo / name).read_bytes()).hexdigest() for name in inputs}
    result = {'platform': platform.system(), 'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'source_sha256': hashes, 'checks': [],
              'scope': 'Production filesystem code, actual temporary folders, Windows junctions and hard links; no game, personal files or GPU.'}
    args.out.parent.mkdir(parents=True, exist_ok=True)

    def run(command, **kwargs):
        r = subprocess.run(list(map(str, command)), cwd=repo, capture_output=True,
                           text=True, encoding='utf-8', timeout=120, **kwargs)
        result['checks'].append({'command': list(map(str, command)), 'exit': r.returncode,
                                 'stdout': r.stdout, 'stderr': r.stderr})
        args.out.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(r.stdout + r.stderr, end='')
        r.check_returncode()
        return r.stdout

    with tempfile.TemporaryDirectory(prefix='sunset-rival-build-') as temp:
        binary = Path(temp) / ('rival-tests.exe' if os.name == 'nt' else 'rival-tests')
        run(['rustc', '--edition=2024', '-D', 'warnings', '--test', 'tools/rival_store_tests.rs', '-o', binary])
        # The collision fixture deliberately reserves the next 16 temporary names.
        output = run([binary, '--test-threads=1', '--nocapture'])
        match = re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored', output)
        assert match, 'No complete passing filesystem result'
        result['passed'] = int(match.group(1))
        assert result['passed'] == (20 if os.name == 'nt' else 18)
        result['source_unchanged'] = hashes == {
            name: hashlib.sha256((repo / name).read_bytes()).hexdigest() for name in inputs}
        assert result['source_unchanged']
        # Keep the published commands usable without recording ephemeral machine paths.
        for check in result['checks']:
            check['command'] = [arg.replace(temp, '<temporary>') for arg in check['command']]
    result['fixtures_removed'] = True
    args.out.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
