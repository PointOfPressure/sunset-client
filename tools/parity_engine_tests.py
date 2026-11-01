"""Compile the production lead/face-place policy against the pinned native kernels."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('engine', type=Path)
    parser.add_argument('--dependency-dir', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    revision = 'ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    assert (args.engine/'.horizon-revision').read_text().strip() == revision
    deps = args.dependency_dir.resolve()
    core, = deps.glob('libhorizon_core-*.rlib')
    protocol, = deps.glob('libhorizon_protocol-*.rlib')
    records = []
    with tempfile.TemporaryDirectory(prefix='sunset-parity-') as scratch:
        executable = Path(scratch)/'lead-faceplace.exe'
        worlds = sorted(deps.glob('libhorizon_world-*.rlib'))
        assert worlds
        for world in worlds:
            command = ['rustc', '--edition=2024', '--test', 'tools/lead_faceplace_tests.rs',
                       '-L', f'dependency={deps}', '--extern', f'horizon_core={core}',
                       '--extern', f'horizon_protocol={protocol}', '--extern', f'horizon_world={world}',
                       '-o', str(executable)]
            result = subprocess.run(command, capture_output=True, text=True)
            records.append({'command': command, 'exit': result.returncode,
                            'stdout': result.stdout, 'stderr': result.stderr})
            if result.returncode == 0:
                break
        if result.returncode == 0:
            result = subprocess.run([str(executable)], capture_output=True, text=True)
            records.append({'command': [str(executable)], 'exit': result.returncode,
                            'stdout': result.stdout, 'stderr': result.stderr})
    sources = [Path('mixins/sunset')/(name+'.rs') for name in
               ['lead_policy', 'faceplace_policy', 'durability_patch', 'prediction_native']]
    proof = {'host_revision': revision, 'checks': records,
             'source_sha256': {p.as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
             'scope': 'Production forecast/threshold policies with native collision, registry and wire kernels; no Game or renderer.'}
    args.out.write_text(json.dumps(proof, indent=2)+'\n', encoding='utf-8')
    print(result.stdout + result.stderr)
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
