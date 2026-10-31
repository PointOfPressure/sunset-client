"""Run production combat policies and pinned engine checks without launching a game."""
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
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--dependency-dir', type=Path, help='Pinned native protocol build dependencies for wire round-trip tests')
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    records = []
    args.out.parent.mkdir(parents=True, exist_ok=True)

    def run(name, command):
        result = subprocess.run(list(map(str, command)), cwd=repo, capture_output=True,
                                text=True, encoding='utf-8', timeout=180)
        records.append({'name': name, 'command': list(map(str, command)),
                        'exit': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
        args.out.write_text(json.dumps({'checks': records}, indent=2)+'\n', encoding='utf-8')
        print(f'{name}: exit {result.returncode}', flush=True)
        if result.returncode:
            print(result.stdout + result.stderr)
            raise SystemExit(result.returncode)
        return result.stdout

    with tempfile.TemporaryDirectory(prefix='sunset-combat-tests-') as directory:
        temporary = Path(directory)
        foundation = temporary/'foundation.json'
        run('explosive foundation', [sys.executable, 'tools/explosive_tests.py',
                                     args.engine.resolve(), '--out', foundation])
        foundation_count = json.loads(foundation.read_text(encoding='utf-8'))['passed_tests']
        for name in ['combat_lease', 'combat_cleanup', 'macro_policy', 'melee_policy', 'inventory_policy',
                     'loot_policy', 'crystal_setup_policy', 'packet_policy', 'sword_policy', 'crystal_bot_policy', 'city_policy', 'roof_scaffold']:
            executable = temporary/(name+'.exe' if sys.platform == 'win32' else name)
            run(name+' compile', ['rustc', '--edition=2024', '--test',
                                  'tools/'+name+'_tests.rs', '-o', executable])
            run(name, [executable])
        run('native Aura base preview and ray', [sys.executable, 'tools/aura_base_engine_tests.py', args.engine.resolve()])
        run('Aura setup suspend and reset lifecycle', [sys.executable, 'tools/aura_setup_lifecycle_tests.py'])
        run('native menu SWAP', [sys.executable, 'tools/inventory_engine_tests.py', args.engine.resolve()])
        run('native packet predicates', [sys.executable, 'tools/packet_engine_tests.py', args.engine.resolve()])
        run('native City mining', [sys.executable, 'tools/city_engine_tests.py', args.engine.resolve()])
        run('native roof visibility', [sys.executable, 'tools/roof_engine_tests.py', args.engine.resolve()])
        run('native Trap placement visibility', [sys.executable, 'tools/trap_visibility_tests.py', args.engine.resolve()])
        run('exact City ownership', [sys.executable, 'tools/city_ownership_tests.py'])
        run('GUI controller edit intent', [sys.executable, 'tools/gui_intent_tests.py'])
        run('native cleanup schema', [sys.executable, 'tools/combat_cleanup_schema_tests.py', args.engine.resolve()])
        run('loot integration gates', [sys.executable, 'tools/loot_integration_tests.py'])
        run('macro protected quick press', [sys.executable, 'tools/loot_press_tests.py'])
        if args.dependency_dir:
            run('native loot identity and wire', [sys.executable, 'tools/loot_engine_tests.py', args.engine.resolve(), '--dependency-dir', args.dependency_dir.resolve()])
            run('native inventory wire round-trip', [sys.executable, 'tools/inventory_protocol_fields.py', args.engine.resolve(), '--dependency-dir', args.dependency_dir.resolve()])
    source_hashes = {p.relative_to(repo).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                     for p in sorted((repo/'mixins/sunset').glob('*.rs'))}
    count = foundation_count + sum(int(n) for r in records if not r['name'].endswith('compile')
                     for n in re.findall(r'test result: ok\. (\d+) passed', r['stdout']))
    # The foundation runner summarizes its six harnesses rather than exposing
    # their Rust output; its expected count is verified from its own summary.
    assert f'{foundation_count} tests passed' in records[0]['stdout']
    source_routes = sum(int(n) for r in records for n in re.findall(r'source-route contract: (\d+) passed', r['stdout']))
    count += source_routes
    args.out.write_text(json.dumps({'passed': count, 'source_route_contract_checks': source_routes, 'checks': records,
        'native_host_revision': 'ba6344a59cb9e755b730dcf02285fc0d33b97d3c',
        'source_sha256': source_hashes,
        'scope': 'Renderer-free production policies and pinned native kernels; no game launched.'},
        indent=2)+'\n', encoding='utf-8')
    print(f'{count} renderer-free checks passed', flush=True)


if __name__ == '__main__':
    main()
