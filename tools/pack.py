#!/usr/bin/env python3
# Packs the mod as dist/sunset-<version>.hnmod: horizon-native.json and
# mixins/sunset/ (only .rs and .toml), as Horizon's launcher reads it. Debug
# Report's commit line gets the commit packed (with -dirty for local changes).
import json, pathlib, subprocess, zipfile

root = pathlib.Path(__file__).resolve().parent.parent
manifest = json.loads((root / "horizon-native.json").read_text())
out = root / "dist" / f"sunset-{manifest['version']}.hnmod"
out.parent.mkdir(exist_ok=True)
git = lambda *a: subprocess.run(["git", "-C", str(root), *a], capture_output=True, text=True).stdout.strip()
commit = (git("rev-parse", "--short", "HEAD") or "unknown") + ("-dirty" if git("status", "--porcelain") else "")
UNPACKED = 'const COMMIT: &str = "unpacked";'
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.write(root / "horizon-native.json", "horizon-native.json")
    for f in sorted((root / "mixins" / "sunset").iterdir()):
        if f.name == "debug.rs":
            text = f.read_text()
            assert UNPACKED in text, "debug.rs lost its COMMIT line"
            z.writestr(f"mixins/sunset/{f.name}", text.replace(UNPACKED, f'const COMMIT: &str = "{commit}";'))
        elif f.suffix in (".rs", ".toml"):
            z.write(f, f"mixins/sunset/{f.name}")
(out.parent / "sunset.hnmod").write_bytes(out.read_bytes())
print(out)
