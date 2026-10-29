#!/usr/bin/env python3
# Packs the mod as dist/delta-<version>.hnmod: horizon-native.json and
# mixins/delta/ (only .rs and .toml), as Horizon's launcher reads it.
import json, pathlib, zipfile

root = pathlib.Path(__file__).resolve().parent.parent
manifest = json.loads((root / "horizon-native.json").read_text())
out = root / "dist" / f"delta-{manifest['version']}.hnmod"
out.parent.mkdir(exist_ok=True)
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.write(root / "horizon-native.json", "horizon-native.json")
    for f in sorted((root / "mixins" / "delta").iterdir()):
        if f.suffix in (".rs", ".toml"):
            z.write(f, f"mixins/delta/{f.name}")
(out.parent / "delta.hnmod").write_bytes(out.read_bytes())
print(out)
