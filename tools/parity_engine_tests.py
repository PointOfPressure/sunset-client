"""Compile the production lead/face-place policy against the pinned native kernels."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
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
    jump_proof = None
    with tempfile.TemporaryDirectory(prefix='sunset-parity-') as scratch:
        worlds = sorted(deps.glob('libhorizon_world-*.rlib'))
        assert worlds
        outputs=[]
        for case in ['lead_faceplace', 'bot_native']:
            executable=Path(scratch)/(case+'.exe')
            for world in worlds:
                command=['rustc','--edition=2024','--test','tools/'+case+'_tests.rs',
                         '-L',f'dependency={deps}','--extern',f'horizon_core={core}',
                         '--extern',f'horizon_protocol={protocol}','--extern',f'horizon_world={world}',
                         '-o',str(executable)]
                result=subprocess.run(command,capture_output=True,text=True)
                records.append({'command':command,'exit':result.returncode,
                                'stdout':result.stdout,'stderr':result.stderr})
                if result.returncode==0:break
            if result.returncode==0:
                result=subprocess.run([str(executable)],capture_output=True,text=True)
                records.append({'command':[str(executable)],'exit':result.returncode,
                                'stdout':result.stdout,'stderr':result.stderr})
            outputs.append(result.stdout+result.stderr)
            if result.returncode:break
        if result.returncode == 0:
            jump_out = Path(scratch)/'jump-environment.json'
            command = [sys.executable, 'tools/jump_environment_tests.py', str(args.engine.resolve()),
                       '--dependency-dir', str(deps), '--out', str(jump_out)]
            result = subprocess.run(command, capture_output=True, text=True)
            records.append({'command':command,'exit':result.returncode,
                            'stdout':result.stdout,'stderr':result.stderr})
            outputs.append(result.stdout+result.stderr)
            if jump_out.exists():jump_proof=json.loads(jump_out.read_text(encoding='utf-8'))
    sources = [Path('mixins/sunset')/(name+'.rs') for name in
               ['lead_policy', 'faceplace_policy', 'durability_patch', 'prediction_native', 'bot_tactics_policy', 'bot_packet_policy']]
    proof = {'host_revision': revision, 'checks': records,
             'source_sha256': {p.as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
             'jump_environment': jump_proof,
             'scope': 'Production forecast/threshold policies and local-motion bridge with native collision, registry and wire kernels; no Game or renderer.'}
    args.out.write_text(json.dumps(proof, indent=2)+'\n', encoding='utf-8')
    print('\n'.join(outputs))
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
