#!/usr/bin/env python3
# Cuts Inter (SIL Open Font License) down to the characters Solstice draws and writes it as a Rust byte array,
# mixins/solstice/font_data.rs. Usage: font.py [path/to/Inter-Medium.ttf]
import subprocess, sys, os, tempfile

src = sys.argv[1] if len(sys.argv) > 1 else "/usr/share/fonts/truetype/inter-zorin-os/Inter-Medium.ttf"
here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
unicodes = "U+0020-007E,U+00A0-00FF,U+2013,U+2014,U+2018,U+2019,U+201C,U+201D,U+2022,U+2026,U+2190-2193,U+2212,U+00D7"
with tempfile.TemporaryDirectory() as d:
    out = os.path.join(d, "ui.ttf")
    subprocess.run(["pyftsubset", src, f"--unicodes={unicodes}", "--layout-features=", "--no-hinting",
                    "--drop-tables+=GSUB,GPOS,GDEF,kern,STAT,fvar,avar,gvar,HVAR,MVAR", f"--output-file={out}"], check=True)
    data = open(out, "rb").read()
os.makedirs(os.path.join(here, "assets"), exist_ok=True)
open(os.path.join(here, "assets", "ui.ttf"), "wb").write(data)
lines = ["// Inter Medium (SIL Open Font License 1.1), cut down by tools/font.py. Solstice's menu and HUD font.",
         "pub static UI: &[u8] = &["]
for i in range(0, len(data), 24):
    lines.append("    " + ", ".join(str(b) for b in data[i:i + 24]) + ",")
lines.append("];")
open(os.path.join(here, "mixins", "solstice", "font_data.rs"), "w").write("\n".join(lines) + "\n")
print(len(data), "bytes")
