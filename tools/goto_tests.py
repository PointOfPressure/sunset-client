"""Run the command parser's real Rust tests without compiling the game."""
from pathlib import Path
import argparse
import subprocess
import tempfile

p = argparse.ArgumentParser()
p.add_argument('--rustc', default='rustc')
a = p.parse_args()
source = Path(__file__).resolve().parents[1] / 'mixins/sunset/goto_args.rs'
with tempfile.TemporaryDirectory(prefix='sunset-goto-') as folder:
    folder = Path(folder)
    wrapper = folder / 'tests.rs'
    wrapper.write_text(f'#[path = r"{source}"] mod args;\n', encoding='utf-8')
    binary = folder / 'tests.exe'
    subprocess.run([a.rustc, '--edition=2024', '--test', str(wrapper), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
