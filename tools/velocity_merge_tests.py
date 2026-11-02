"""Test the production config/profile migration with the installed Rust compiler."""
from pathlib import Path
import argparse
import subprocess
import tempfile

p = argparse.ArgumentParser()
p.add_argument('--rustc', default='rustc')
a = p.parse_args()
source = (Path(__file__).resolve().parents[1] / 'mixins/sunset/runtime.rs').read_text(encoding='utf-8')

def block(needle):
    start = source.index(needle)
    end = source.index('{', start) + 1
    depth = 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end]

with tempfile.TemporaryDirectory(prefix='sunset-velocity-') as folder:
    folder = Path(folder)
    tests = folder / 'tests.rs'
    tests.write_text(block('fn merged_modules(') + '\n' + block('mod velocity_merge_tests {'), encoding='utf-8')
    binary = folder / 'tests.exe'
    subprocess.run([a.rustc, '--edition=2024', '--test', str(tests), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
