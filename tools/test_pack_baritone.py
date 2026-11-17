#!/usr/bin/env python3
"""Focused archive/linkage/notice regressions; fixtures and temporary outputs remain in dist/."""
import hashlib
import json
import pathlib
import re
import shutil
import tempfile
import tomllib
import unittest
import zipfile

import pack


class Packages(unittest.TestCase):
    def setUp(self):
        scratch = pack.ROOT / "dist" / "pack-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(prefix="baritone-", dir=scratch)
        self.addCleanup(self.directory.cleanup)
        self.root = pathlib.Path(self.directory.name)
        self.horizon, self.mixins = "a" * 40, "b" * 40
        self.write("horizon-native.json", json.dumps({"schema": 1, "id": "sunset", "version": "1.2.1", "game": "26.3", "horizon": self.horizon, "mixins": self.mixins}))
        self.write("mixins/sunset/debug.rs", pack.UNPACKED)
        self.write("mixins/sunset/horizon-mixin.toml", f'[mod]\nid="sunset"\nversion="1.2.1"\n[compatibility]\nhorizon_revision="{self.horizon}"\n')
        self.write("mixins/sunset/seedcracker.LICENSE", b"seed licence\r\n\x00")
        self.write("mixins/sunset/seedcracker.NOTICE.md", b"seed attribution\r\n")

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content if isinstance(content, bytes) else content.encode("utf-8"))
        return path

    def companion(self):
        self.write("mixins/baritone/horizon-native.json", json.dumps({"schema": 1, "id": "sunset_baritone", "version": "1.1.52", "game": "26.3", "horizon": self.horizon, "mixins": self.mixins}))
        self.write("mixins/baritone/horizon-mixin.toml", f'[mod]\nid="sunset_baritone"\nversion="1.1.52"\n[compatibility]\nhorizon_revision="{self.horizon}"\n[[crate]]\nid="loader"\npath="loader"\n')
        self.write("mixins/baritone/loader/Cargo.toml", '[package]\nname="sunset-baritone-loader"\nversion="1.0.0"\n[dependencies]\nlibloading="0.8.9"\n')
        self.write("mixins/baritone/loader/src/lib.rs", 'pub fn bundled() -> &\'static [u8] { include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../native-data/sunset_baritone/engine/x86_64-unknown-linux-gnu/libsunset_baritone_engine.so")) }\n')
        self.write("mixins/baritone/LICENSE", b"MIT loader licence\r\n")
        for name in ("mixins/baritone/interface/abi-v1.rs", "mixins/baritone/loader/src/abi.rs", "baritone-engine/src/abi.rs"):
            self.write(name, b"// independently authored MIT ABI\r\n")
        self.write("mixins/baritone/register.rs", "// MIT registration\n")
        self.write("baritone-engine/Cargo.toml", '[package]\nname="sunset-baritone-engine"\nversion="1.1.52"\nlicense-file="LICENSE"\n')
        self.write("baritone-engine/Cargo.lock", "version = 4\n")
        self.write("baritone-engine/src/lib.rs", "// LGPL implementation\n")
        self.write("baritone-engine/tests/data/oracle.bin", b"oracle fixture")
        self.write("baritone-engine/dependency-licenses/serde_json/LICENSE-MIT", b"exact dependency grant\r\n\xff")
        for i, name in enumerate(pack.NOTICES):
            notice = b"unaltered upstream notice\r\n" + bytes([i, 0xff])
            self.write("baritone-engine/" + name, notice)
            self.write("mixins/baritone/notices/" + name, notice)
        self.write("tools/pack.py", pathlib.Path(pack.__file__).read_bytes())
        self.write("docs/baritone.md", "# Replacement and source\n")
        dll = bytearray(256)
        dll[:2] = b"MZ"
        dll[60:64] = (128).to_bytes(4, "little")
        dll[128:134] = b"PE\0\0\x64\x86"
        dll[150:152] = (0x2000).to_bytes(2, "little")
        so = b"\x7fELF\x02\x01" + bytes(10) + b"\x03\x00\x3e\x00" + bytes(64)
        self.engines = {"x86_64-unknown-linux-gnu": so, "x86_64-pc-windows-gnu": bytes(dll)}
        return self.engines

    def test_plain_pack_needs_no_companion_and_preserves_seed_notices(self):
        outputs = pack.package(self.root)
        self.assertEqual([path.name for path in outputs], ["sunset-1.2.1.hnmod"])
        self.assertEqual(outputs[0].read_bytes(), (self.root / "dist/sunset.hnmod").read_bytes())
        with zipfile.ZipFile(outputs[0]) as archive:
            self.assertFalse(any("baritone" in name for name in archive.namelist()))
            for name in ("seedcracker.LICENSE", "seedcracker.NOTICE.md"):
                self.assertEqual(archive.read("mixins/sunset/" + name), (self.root / "mixins/sunset" / name).read_bytes())

    def test_companion_and_source_are_complete_with_byte_exact_notices(self):
        engines = self.companion()
        for name in ("baritone-engine/target/debug/secret.pdb", "baritone-engine/target/debug/build.rs", "mixins/baritone/loader/target/cache.rs", "baritone-engine/tmp/stale.rs", "baritone-engine/resources/game.json", "baritone-engine/crash.log"):
            self.write(name, "must not ship")
        outputs = pack.package(self.root, with_baritone=True, engines=self.engines)
        self.assertEqual(len(outputs), 2)
        prefix = "mixins/sunset/baritone/"
        with zipfile.ZipFile(outputs[0]) as sunset, zipfile.ZipFile(outputs[0]) as companion, zipfile.ZipFile(outputs[1]) as source:
            self.assertEqual(json.loads(companion.read("horizon-native.json"))["id"], "sunset")
            for name, path in pack.notice_files(self.root / "baritone-engine"):
                expected = path.read_bytes()
                self.assertEqual(source.read("baritone-engine/" + name), expected)
                if name.startswith("dependency-licenses/"):
                    combined = sunset.read("mixins/sunset/data/notices/baritone/DEPENDENCY-LICENSES.txt")
                    self.assertIn(name.encode(), combined)
                    self.assertIn(expected, combined)
                else:
                    self.assertEqual(sunset.read("mixins/sunset/data/notices/baritone/" + name), expected)
            for target, payload in engines.items():
                self.assertEqual(companion.read(f"mixins/sunset/data/baritone/engine/{target}/{pack.ENGINES[target]}"), payload)
            self.assertIn(prefix + "loader/Cargo.toml", companion.namelist())
            loader = companion.read(prefix + "loader/src/lib.rs")
            self.assertIn(b"native-data/sunset/baritone/engine/", loader)
            self.assertNotIn(b"native-data/sunset_baritone/engine/", loader)
            # The rewritten include_bytes path must select the exact bytes staged
            # from the root data/ directory; the corresponding source stays intact.
            literal = re.search(rb'"(/\.\./\.\./\.\./native-data/[^\"]+)"', loader).group(1)
            staged_relative = literal.decode().split("/native-data/sunset/", 1)[1]
            self.assertEqual(companion.read("mixins/sunset/data/" + staged_relative), engines["x86_64-unknown-linux-gnu"])
            self.assertEqual(source.read("mixins/baritone/loader/src/lib.rs"),
                             (self.root / "mixins/baritone/loader/src/lib.rs").read_bytes())
            self.assertNotIn("baritone-engine/Cargo.toml", companion.namelist())
            for name in ("baritone-engine/Cargo.lock", "baritone-engine/tests/data/oracle.bin", "mixins/baritone/interface/abi-v1.rs", "docs/baritone.md"):
                self.assertIn(name, source.namelist())
            self.assertFalse(any("/target/" in name or name.endswith((".dll", ".pdb", ".log")) for name in source.namelist()))
            self.assertEqual({lib["target"]: lib["sha256"] for lib in json.loads(source.read("ENGINE-BINARY.json"))["libraries"]},
                             {target: hashlib.sha256(payload).hexdigest() for target, payload in engines.items()})

    def test_invalid_linkage_notice_and_architecture_fail_before_repacking(self):
        engines = self.companion()
        outputs = pack.package(self.root)
        original = outputs[0].read_bytes()
        for name, content, reason in (
            ("mixins/baritone/loader/Cargo.toml", '[package]\nname="loader"\nversion="1.0.0"\n[dependencies]\nsunset-baritone-engine={path="../../../baritone-engine"}\n', "cannot depend"),
            ("mixins/baritone/loader/Cargo.toml", '[package]\nname="loader"\nversion="1.0.0"\n[target.\'cfg(windows)\'.dependencies]\nengine={package="sunset-baritone-engine",version="1.1.52"}\n', "cannot depend"),
            ("mixins/baritone/notices/NOTICE.md", "missing attribution", "notice differs"),
            ("mixins/baritone/loader/src/abi.rs", "incompatible ABI", "ABI declarations differ"),
        ):
            with self.subTest(name=name):
                path = self.root / name
                before = path.read_bytes()
                self.write(name, content)
                with self.assertRaisesRegex(ValueError, reason):
                    pack.package(self.root, with_baritone=True, engines=self.engines)
                self.assertEqual(outputs[0].read_bytes(), original)
                path.write_bytes(before)
        for target, reason in (("x86_64-pc-windows-gnu", "not an x86_64 DLL"), ("x86_64-unknown-linux-gnu", "not an x86_64 ELF")):
            with self.subTest(target=target):
                with self.assertRaisesRegex(ValueError, reason):
                    pack.package(self.root, with_baritone=True, engines={**engines, target: b"not a library"})
                self.assertEqual(outputs[0].read_bytes(), original)
        with self.assertRaisesRegex(ValueError, "exactly"):
            pack.package(self.root, with_baritone=True, engines={"x86_64-pc-windows-gnu": engines["x86_64-pc-windows-gnu"]})

    def test_launcher_id_and_file_allowlist_regressions(self):
        self.companion()
        package = pack.package(self.root, with_baritone=True, engines=self.engines)[0]
        with zipfile.ZipFile(package, "a", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("mixins/sunset/baritone/interface/protocol-v1.md", "must be under data or use README prefix")
        with self.assertRaisesRegex(ValueError, "Horizon rejects native archive path"):
            pack.validate_native_archive(package)
        # The transformer's accepted IDs are broader than the launcher's installable IDs.
        broken = self.root / "invalid-id.hnmod"
        with zipfile.ZipFile(broken, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("horizon-native.json", json.dumps({"schema": 1, "id": "sunset-baritone", "version": "1.0.0", "game": "26.3", "horizon": self.horizon, "mixins": self.mixins}))
        with self.assertRaisesRegex(ValueError, "underscore/alphanumeric id"):
            pack.validate_native_archive(broken)

    def test_full_runtime_sources_hooks_commands_and_permission_text_ship(self):
        self.companion()
        upstream = pack.ROOT / "mixins/baritone/upstream"
        shutil.copytree(upstream, self.root / "mixins/baritone/upstream")
        original = (upstream / "horizon-mixin.toml").read_text(encoding="utf-8")
        declarations = original[original.index("[[module]]"):]
        declarations = re.sub(r'(?m)^id = "([^"]+)"', r'id = "full_\1"', declarations)
        declarations = re.sub(r'(?m)^(source|helper) = "([^"]+)"', r'\1 = "upstream/\2"', declarations)
        manifest = self.root / "mixins/baritone/horizon-mixin.toml"
        manifest.write_text(manifest.read_text() + "\n" + declarations, encoding="utf-8")
        package, source = pack.package(self.root, with_baritone=True, engines=self.engines)
        with zipfile.ZipFile(package) as native, zipfile.ZipFile(source) as corresponding:
            parsed = tomllib.loads(manifest.read_text())
            combined = tomllib.loads(native.read("mixins/sunset/horizon-mixin.toml").decode())
            self.assertLessEqual(len(native.namelist()), 512)
            self.assertTrue(all(name == "horizon-native.json" or name.startswith("mixins/sunset/")
                                for name in native.namelist()))
            # The single installed Sunset manifest must actually load those files.
            for key, field in (("module", "source"), ("injection", "helper")):
                merged = {entry["id"]: entry for entry in combined.get(key, [])}
                for entry in parsed.get(key, []):
                    self.assertEqual(merged["baritone_" + entry["id"]][field], "baritone/" + entry[field])
            # Every active native source and hook must exist in both deliverables.
            for key, field in (("module", "source"), ("injection", "helper")):
                for entry in parsed.get(key, []):
                    name = entry[field]
                    expected = (self.root / "mixins/baritone" / name).read_bytes()
                    self.assertEqual(native.read("mixins/sunset/baritone/" + name), expected)
                    self.assertEqual(corresponding.read("mixins/baritone/" + name), expected)
            for relative in ("src/process/build.rs", "src/process/farm.rs", "src/process/follow.rs",
                             "src/process/explore.rs", "src/process/elytra.rs", "src/selection/selection.rs",
                             "src/command/manager.rs", "src/game/settings_ui.rs", "src/ui/settings_screen.rs"):
                expected = (upstream / relative).read_bytes()
                self.assertEqual(native.read("mixins/sunset/baritone/upstream/" + relative), expected)
            permission = (upstream / "LICENSE").read_bytes()
            self.assertEqual(native.read("mixins/sunset/data/docs/baritone/upstream/LICENSE"), permission)
            self.assertEqual(corresponding.read("mixins/baritone/upstream/LICENSE"), permission)

    def test_full_runtime_manifest_preserves_original_hook_and_module_inventory(self):
        companion = pack.ROOT / "mixins/baritone"
        original = tomllib.loads((companion / "upstream/horizon-mixin.toml").read_text())
        active = tomllib.loads((companion / "horizon-mixin.toml").read_text())
        modules = {m["id"]: m for m in active["module"]}
        hooks = {h["id"]: h for h in active["injection"]}
        for module in original["module"]:
            copied = modules["full_" + module["id"]]
            self.assertEqual(copied["name"], module["name"])
            self.assertEqual(copied["file"], module["file"])
            self.assertEqual(copied["source"], "upstream/" + module["source"])
            self.assertTrue((companion / copied["source"]).is_file())
        for hook in original["injection"]:
            copied = hooks["full_" + hook["id"]]
            self.assertEqual(copied["kind"], hook["kind"])
            target = hook["target"].replace("::game::tweaks::Game", "::game::extras::sorting::Game")
            self.assertEqual(copied["target"], target)
            self.assertEqual(copied["helper"], "upstream/" + hook["helper"])
            self.assertEqual(copied.get("bindings", []), hook.get("bindings", []))
            self.assertEqual(copied.get("cancellable", False), hook.get("cancellable", False))
            self.assertTrue((companion / copied["helper"]).is_file())


if __name__ == "__main__":
    unittest.main()
