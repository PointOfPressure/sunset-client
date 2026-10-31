"""Run reproducible renderer-free aura tests; optionally save their raw proof.

python tools/explosive_tests.py /path/to/pinned/horizon [--out report.json]
Native compilation and live worlds are separate checks, not implied here.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('engine', type=Path)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    report = {'kind': 'standalone-original-explosive-aura-tests',
              'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'checks': [], 'limitations': ['No renderer, actual ray/world queries, packet exchange or multiplayer exercised.']}

    def run(label, command, *, tests=False):
        result = subprocess.run(list(map(str, command)), cwd=root, capture_output=True, text=True, encoding='utf-8')
        matches = re.findall(r'test result: ok\. (\d+) passed', result.stdout)
        count = sum(map(int, matches))
        report['checks'].append({'name': label, 'command': list(map(str, command)),
                                 'exit_code': result.returncode, 'passed_tests': count,
                                 'stdout': result.stdout, 'stderr': result.stderr})
        print(f'{label}: ' + (f'{count} passed' if tests and not result.returncode else f'exit {result.returncode}'), flush=True)
        if result.returncode or (tests and not count):
            print(result.stdout + result.stderr, file=sys.stderr)
            raise RuntimeError(label)

    try:
        with tempfile.TemporaryDirectory(prefix='sunset-explosive-tests-') as scratch:
            for label, source in [('foundation', 'explosive_foundation_tests.rs'), ('anchor ordering', 'anchor_aura_tests.rs'),
                                  ('shared opportunity choice', 'explosive_choice_tests.rs')]:
                binary = Path(scratch) / (Path(source).stem + ('.exe' if sys.platform == 'win32' else ''))
                run(label + ' compile', ['rustc', '--edition=2024', '--test', '-O', root/'tools'/source, '-o', binary])
                run(label, [binary], tests=True)
            for label, script, extra in [
                ('crystal state and geometry', 'crystal_aura_tests.py', []),
                ('native damage equivalence', 'explosive_engine_equivalence.py', [args.engine.resolve()]),
                ('rotation ownership', 'explosive_turn_tests.py', []),
            ]:
                run(label, [sys.executable, root/'tools'/script, *extra], tests=True)
    finally:
        report['passed_tests'] = sum(c['passed_tests'] for c in report['checks'])
        report['source_sha256'] = {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted((root/'mixins/sunset').glob('*.rs'))
            if p.name.startswith(('anchor_', 'crystal_', 'explosive_')) or p.name == 'runtime.rs'}
        if args.out:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f"{report['passed_tests']} tests passed", flush=True)


if __name__ == '__main__':
    main()
