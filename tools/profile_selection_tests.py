"""Run the production profile-selection lifecycle tests with the installed Rust compiler."""
from pathlib import Path
import argparse
import json
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--rustc', default='rustc')
args = parser.parse_args()
source = Path(__file__).resolve().parents[1] / 'mixins/sunset/profiles.rs'
with tempfile.TemporaryDirectory(prefix='sunset-profile-selection-') as folder:
    folder = Path(folder)
    tests = folder / 'tests.rs'
    tests.write_text('#[allow(dead_code)]\n#[path = '+json.dumps(source.as_posix())+']\nmod profiles;\n', encoding='utf-8')
    binary = folder / 'tests.exe'
    subprocess.run([args.rustc, '--edition=2024', '--test', str(tests), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
