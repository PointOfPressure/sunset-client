#!/usr/bin/env python3
# Packs the mod as dist/sunset-<version>.hnmod: horizon-native.json and
# mixins/sunset/ (only .rs and .toml), as Horizon's launcher reads it.
import json, pathlib, zipfile

root = pathlib.Path(__file__).resolve().parent.parent
manifest = json.loads((root / "horizon-native.json").read_text())
out = root / "dist" / f"sunset-{manifest['version']}.hnmod"
out.parent.mkdir(exist_ok=True)
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.write(root / "horizon-native.json", "horizon-native.json")
    for f in sorted((root / "mixins" / "sunset").iterdir()):
        if f.suffix in (".rs", ".toml"):
            z.write(f, f"mixins/sunset/{f.name}")
(out.parent / "sunset.hnmod").write_bytes(out.read_bytes())
print(out)
