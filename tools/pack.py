#!/usr/bin/env python3
# Packs dist/sunset-<version>.hnmod. With Baritone (the default when mixins/baritone exists) it first builds the LGPL
# engine from baritone-engine/ for Linux and Windows (cargo, the x86_64-pc-windows-gnu target and a mingw-w64 linker)
# and ships both libraries beside the full runtime, plus baritone-engine-<version>-source.zip.
import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import shutil
import tempfile
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Rust target -> the library file the loader looks for on that system.
ENGINES = {"x86_64-unknown-linux-gnu": "libsunset_baritone_engine.so", "x86_64-pc-windows-gnu": "sunset_baritone_engine.dll"}
NOTICES = ("LICENSE", "COPYING", "NOTICE.md", "LICENSE-Part-2.jpg", "UPSTREAM.md", "CHANGES.md", "DEPENDENCIES.md")
UNPACKED = 'const COMMIT: &str = "unpacked";'
EXCLUDED_DIRS = {"target", ".git", ".cache", ".local", "__pycache__", "node_modules", "dist", "tmp", "temp", "saves", "resources", "native-data", ".horizon-mod-data"}
EXCLUDED_SUFFIXES = {".dll", ".so", ".dylib", ".pdb", ".lib", ".exp", ".exe", ".pyc", ".tmp", ".log"}
THIRD_PARTY = """Baritone engine: horizon-baritone by luhnox, based on Baritone 1.21.4 by leijurv and contributors.
The separately replaceable engine is LGPL-3.0 under its upstream terms; its rights are separate from Sunset's licence.
The loader and ABI are MIT. See the accompanying notices and baritone-engine-<version>-source.zip.
Replacing the engine library and building it from source: docs/baritone.md in the source archive.
"""


def source_files(directory):
    for current, dirs, files in os.walk(directory):
        dirs[:] = sorted(name for name in dirs if name not in EXCLUDED_DIRS)
        for name in [*dirs, *files]:
            if (pathlib.Path(current) / name).is_symlink():
                raise ValueError(f"Source archives cannot contain symlinks: {pathlib.Path(current) / name}")
        for name in sorted(files):
            path = pathlib.Path(current) / name
            if path.suffix.lower() not in EXCLUDED_SUFFIXES:
                yield path


def read_toml(path):
    import tomllib
    return tomllib.loads(path.read_text(encoding="utf-8"))


def dependencies(table):
    for key, value in table.items():
        if key in {"dependencies", "dev-dependencies", "build-dependencies"}:
            yield from value.items()
        elif isinstance(value, dict):
            yield from dependencies(value)


def notice_files(engine):
    for name in NOTICES:
        yield name, engine / name
    for path in source_files(engine / "dependency-licenses"):
        yield path.relative_to(engine).as_posix(), path


def native_allowed(rest):
    # Horizon 9f42064 horizon-mods::index::native::{allowed,in_data,text}.
    parts = rest.split("/")
    data = len(parts) >= 2 and (parts[0] == "data" or (len(parts) >= 3 and parts[1] == "data"))
    text = len(parts) <= 2 and (parts[-1].startswith(("README", "LICENSE", "LICENCE", "COPYING", "NOTICE")) or parts[-1] == ".gitignore")
    return rest.endswith((".rs", ".toml")) or data or text


def validate_native_archive(path):
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)) or len(names) > 512 or sum(entry.file_size for entry in entries) > 64 * 1024 * 1024:
            raise ValueError(f"Native archive exceeds Horizon limits or contains duplicates: {path}")
        metadata = json.loads(archive.read("horizon-native.json"))
        if (metadata.get("schema") != 1 or not re.fullmatch(r"[a-z0-9_]{1,64}", metadata.get("id", ""))
                or not metadata.get("version") or not metadata.get("game")
                or any(not re.fullmatch(r"[a-f0-9]{40}", metadata.get(key, "")) for key in ("horizon", "mixins"))):
            raise ValueError("Horizon requires schema 1, an underscore/alphanumeric id and exact revision pins")
        prefix = f"mixins/{metadata['id']}/"
        for name in names:
            if name == "horizon-native.json":
                continue
            if ("\\" in name or ":" in name or any(part in {"", ".", ".."} for part in name.split("/"))
                    or not name.startswith(prefix) or not native_allowed(name[len(prefix):])):
                raise ValueError(f"Horizon rejects native archive path: {name}")
        import tomllib
        manifest = tomllib.loads(archive.read(prefix + "horizon-mixin.toml").decode("utf-8"))
        if (manifest["mod"]["id"] != metadata["id"] or manifest["mod"]["version"] != metadata["version"]
                or manifest["compatibility"]["horizon_revision"] != metadata["horizon"]):
            raise ValueError("Native archive manifest/metadata identity or revision mismatch")


def validate_baritone(root, engines):
    companion = root / "mixins" / "baritone"
    engine = root / "baritone-engine"
    metadata = json.loads((companion / "horizon-native.json").read_text(encoding="utf-8"))
    sunset = json.loads((root / "horizon-native.json").read_text(encoding="utf-8"))
    manifest = read_toml(companion / "horizon-mixin.toml")
    # The engine compiles horizon-baritone's one source copy (mixins/baritone/upstream/src) by relative path, so every
    # file it names must exist and be in its source archive: under baritone-engine/ or mixins/baritone/ (not notices/data).
    for included in re.findall(r'#\[path = "([^"]+)"\]', (engine / "src" / "lib.rs").read_text(encoding="utf-8")):
        path = pathlib.Path(os.path.normpath(engine / "src" / included))
        shipped = path.is_relative_to(engine) or (path.is_relative_to(companion) and path.relative_to(companion).parts[0] not in {"notices", "data"})
        if not shipped or not path.is_file():
            raise ValueError(f"Engine source missing or outside its source archive: {included}")
    cargo = read_toml(engine / "Cargo.toml")
    loader = read_toml(companion / "loader" / "Cargo.toml")
    if metadata["id"] != "sunset_baritone" or manifest["mod"]["id"] != metadata["id"]:
        raise ValueError("Companion mod id must be sunset_baritone (pinned Horizon forbids hyphens)")
    if manifest["mod"]["version"] != metadata["version"]:
        raise ValueError("Companion manifest/metadata version mismatch")
    for key in ("game", "horizon", "mixins"):
        if metadata[key] != sunset[key]:
            raise ValueError(f"Companion/Sunset {key} pin mismatch")
    if manifest["compatibility"]["horizon_revision"] != sunset["horizon"]:
        raise ValueError("Companion transformer pin mismatch")
    if [crate["path"] for crate in manifest.get("crate", [])] != ["loader"]:
        raise ValueError("Only the independent MIT loader may be linked into the game")
    for name, dependency in dependencies(loader):
        package = dependency.get("package", name) if isinstance(dependency, dict) else name
        if "baritone" in package.lower() or (isinstance(dependency, dict) and ("path" in dependency or "git" in dependency)):
            raise ValueError("The loader cannot depend on the LGPL engine or a path/git package")
    if (companion / "loader" / "Cargo.lock").exists() or "profile" in loader:
        raise ValueError("A staged loader cannot contain Cargo.lock or Cargo profiles")
    for path in (companion / "loader" / "src").rglob("*.rs"):
        if re.search(r"\b(?:sunset_baritone_engine|horizon_baritone)::", path.read_text(encoding="utf-8")):
            raise ValueError(f"Engine implementation cannot be statically referenced: {path}")
    abi = (companion / "interface" / "abi-v1.rs").read_bytes()
    for path in (engine / "src" / "abi.rs", companion / "loader" / "src" / "abi.rs"):
        if path.read_bytes() != abi:
            raise ValueError(f"ABI declarations differ: {path}")
    if not any(source_files(engine / "dependency-licenses")):
        raise ValueError("Required dependency licence texts missing: baritone-engine/dependency-licenses")
    for name, path in notice_files(engine):
        original = path.read_bytes()
        copy = companion / "notices" / name
        if copy.exists() and copy.read_bytes() != original:
            raise ValueError(f"Companion notice differs from engine: {name}")
    for path in (engine / "Cargo.lock", companion / "LICENSE", root / "docs" / "baritone.md"):
        if not path.is_file():
            raise ValueError(f"Required corresponding-source file missing: {path}")
    if set(engines) != set(ENGINES):
        raise ValueError(f"Engine libraries are needed for exactly {sorted(ENGINES)}")
    for target, payload in engines.items():
        check_engine(target, payload)
    return metadata, cargo["package"]["version"], engines


def check_engine(target, payload):
    if "windows" in target:
        offset = int.from_bytes(payload[60:64], "little") if len(payload) >= 64 and payload[:2] == b"MZ" else -1
        if offset < 0 or payload[offset:offset + 6] != b"PE\0\0\x64\x86" or offset + 24 > len(payload) \
                or not int.from_bytes(payload[offset + 22:offset + 24], "little") & 0x2000:
            raise ValueError(f"The {target} engine is not an x86_64 DLL")
    elif payload[:6] != b"\x7fELF\x02\x01" or payload[16:20] != b"\x03\x00\x3e\x00":
        raise ValueError(f"The {target} engine is not an x86_64 ELF shared library")


def build_engines(root):
    """Builds the engine from source for every shipped target; nothing prebuilt is packed."""
    engine = root / "baritone-engine"
    target_dir = root / "target" / "baritone-engine"
    built = {}
    for target, file in ENGINES.items():
        subprocess.run(["cargo", "build", "--release", "--locked", "--target", target, "--manifest-path", str(engine / "Cargo.toml"),
                        "--target-dir", str(target_dir)], check=True, env={**os.environ, "RUSTFLAGS": "-Dwarnings"})
        built[target] = (target_dir / target / "release" / file).read_bytes()
    return built


def pack_sunset(root, output, manifest, commit, with_baritone=False):
    out = output / f"sunset-{manifest['version']}.hnmod"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.write(root / "horizon-native.json", "horizon-native.json")
        for notice in ("seedcracker.LICENSE", "seedcracker.NOTICE.md"):
            dest = f"mixins/sunset/data/notices/{notice}" if with_baritone else f"mixins/sunset/{notice}"
            archive.write(root / "mixins" / "sunset" / notice, dest)
        provenance = root / "mixins" / "sunset" / "seedcracker.UPSTREAM.md"
        if provenance.is_file():
            archive.write(provenance, "mixins/sunset/NOTICE.seedcracker-UPSTREAM.md")
        for path in sorted((root / "mixins" / "sunset").iterdir()):
            if path.name == "debug.rs":
                text = path.read_text(encoding="utf-8")
                if UNPACKED not in text:
                    raise ValueError("debug.rs lost its COMMIT line")
                archive.writestr(f"mixins/sunset/{path.name}", text.replace(UNPACKED, f'const COMMIT: &str = "{commit}";'))
            elif path.suffix in (".rs", ".toml"):
                archive.write(path, f"mixins/sunset/{path.name}")
        if with_baritone:
            archive.writestr("mixins/sunset/NOTICE.baritone.md", THIRD_PARTY)
            for name, path in notice_files(root / "baritone-engine"):
                archive.write(path, f"mixins/sunset/data/notices/baritone/{name}")
            archive.write(root / "mixins" / "baritone" / "LICENSE", "mixins/sunset/data/notices/baritone/loader.LICENSE")
    if with_baritone:
        validate_native_archive(out)
    (output / "sunset.hnmod").write_bytes(out.read_bytes())
    return out


def builtin_baritone(root, sunset, companion):
    """One installable mod: merge the native source manifests and keep the 512-file limit."""
    prefix = "mixins/sunset/"
    native = read_toml(root / "mixins/baritone/horizon-mixin.toml")
    raw = (root / "mixins/baritone/horizon-mixin.toml").read_text(encoding="utf-8")
    start = min((raw.index(mark) for mark in ("[[crate]]", "[[module]]", "[[injection]]") if mark in raw))
    extension = raw[start:]
    extension = re.sub(r'(?m)^id = "([^"]+)"', r'id = "baritone_\1"', extension)
    extension = re.sub(r'(?m)^(source|helper|path) = "([^"]+)"', r'\1 = "baritone/\2"', extension)
    # Preserve companion ordering after merging under the Sunset mod ID.
    extension = extension.replace('[[injection]]\n', '[[injection]]\npriority = 150\n')
    referenced = {v[k] for section, k in (("module", "source"), ("injection", "helper"))
                  for v in native.get(section, [])}
    with zipfile.ZipFile(sunset) as base, zipfile.ZipFile(companion) as extra:
        files = {name: base.read(name) for name in base.namelist()}
        text = files[prefix+"horizon-mixin.toml"].decode("utf-8")
        files[prefix+"horizon-mixin.toml"] = (text+"\n# Built-in full Baritone runtime and Ore Sim engine.\n"+extension).encode()
        deps = []
        for name in list(files):
            if "/dependency-licenses/" in name:
                deps.append(name.encode()+b"\n"+files.pop(name)+b"\n\n")
        if deps:
            files[prefix+"data/notices/baritone/DEPENDENCY-LICENSES.txt"] = b"".join(deps)
        for name in extra.namelist():
            if name == "horizon-native.json": continue
            rest = name.removeprefix("mixins/sunset_baritone/")
            if rest == "horizon-mixin.toml" or rest.startswith("data/notices/"): continue
            if rest.startswith("upstream/") and rest not in referenced and not rest.endswith(("/LICENSE", "/README.md")):
                continue
            dest = ("data/baritone/"+rest[5:]) if rest.startswith("data/") else "baritone/"+rest
            if not native_allowed(dest): dest = "data/docs/"+dest
            data = extra.read(name)
            if rest == "loader/src/lib.rs":
                data = data.replace(b"native-data/sunset_baritone/engine/", b"native-data/sunset/baritone/engine/")
            files[prefix+dest] = data
    if len(files) > 512:
        raise ValueError(f"Built-in bundle exceeds Horizon's 512 entries: {len(files)}")
    temp = sunset.with_suffix('.building')
    with zipfile.ZipFile(temp, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(files.items()): archive.writestr(name, data)
    os.replace(temp, sunset)
    validate_native_archive(sunset)
    (sunset.parent/"sunset.hnmod").write_bytes(sunset.read_bytes())


def pack_baritone(root, output, metadata, engine_version, engines):
    companion = root / "mixins" / "baritone"
    engine = root / "baritone-engine"
    prefix = f"mixins/{metadata['id']}"
    out = output / f"sunset-baritone-{metadata['version']}.hnmod"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.write(companion / "horizon-native.json", "horizon-native.json")
        for path in source_files(companion):
            relative = path.relative_to(companion)
            if relative.as_posix() in {"horizon-native.json", "THIRD-PARTY.md", "baritone.md"} or relative.parts[0] in {"notices", "data"}:
                continue
            relative = relative.as_posix()
            if not native_allowed(relative):
                relative = "data/docs/" + relative
            archive.write(path, f"{prefix}/{relative}")
        for target, payload in engines.items():
            archive.writestr(f"{prefix}/data/engine/{target}/{ENGINES[target]}", payload)
        archive.writestr(f"{prefix}/NOTICE.third-party.md", THIRD_PARTY)
        for name, path in notice_files(engine):
            archive.write(path, f"{prefix}/data/notices/{name}")
        archive.write(root / "docs" / "baritone.md", f"{prefix}/README.baritone.md")
    validate_native_archive(out)
    source = output / f"baritone-engine-{engine_version}-source.zip"
    with zipfile.ZipFile(source, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in source_files(engine):
            archive.write(path, path.relative_to(root).as_posix())
        for path in source_files(companion):
            relative = path.relative_to(companion)
            if relative.parts[0] not in {"notices", "data"}:
                archive.write(path, path.relative_to(root).as_posix())
        for name, path in notice_files(engine):
            archive.write(path, f"mixins/baritone/notices/{name}")
        for name in ("tools/pack.py", "docs/baritone.md"):
            archive.write(root / name, name)
        archive.writestr("ENGINE-BINARY.json", json.dumps({"engine_version": engine_version, "source": source.name, "libraries": [
            {"target": target, "file": ENGINES[target], "sha256": hashlib.sha256(payload).hexdigest()} for target, payload in engines.items()]},
            indent=2) + "\n")
    with zipfile.ZipFile(out) as archive, zipfile.ZipFile(source) as sources:
        for name, path in notice_files(engine):
            expected = path.read_bytes()
            if archive.read(f"{prefix}/data/notices/{name}") != expected or sources.read(f"baritone-engine/{name}") != expected:
                raise ValueError(f"Packaged notice is not byte-identical: {name}")
        for target, payload in engines.items():
            if archive.read(f"{prefix}/data/engine/{target}/{ENGINES[target]}") != payload:
                raise ValueError(f"Packaged {target} engine differs from the one built")
        if any(name.endswith(('.pdb', '.lib', '.exp')) or name.startswith("baritone-engine/") for name in archive.namelist()):
            raise ValueError("Companion includes link inputs or engine implementation")
    return out, source


def package(root=ROOT, with_baritone=None, engines=None):
    if with_baritone is None:
        with_baritone = (root / "mixins/baritone/full.rs").is_file()
    manifest = json.loads((root / "horizon-native.json").read_text(encoding="utf-8"))
    output = root / "dist"
    output.mkdir(exist_ok=True)
    bundle = None
    if with_baritone:
        bundle = validate_baritone(root, engines if engines is not None else build_engines(root))
    git = lambda *args: subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True).stdout.strip()
    commit = (git("rev-parse", "--short", "HEAD") or "unknown") + ("-dirty" if git("status", "--porcelain") else "")
    outputs = [pack_sunset(root, output, manifest, commit, with_baritone)]
    if bundle:
        with tempfile.TemporaryDirectory(prefix="baritone-pack-", dir=output) as scratch:
            companion, sources = pack_baritone(root, pathlib.Path(scratch), *bundle)
            builtin_baritone(root, outputs[0], companion)
            source_out = output / sources.name
            shutil.copy2(sources, source_out)
            outputs.append(source_out)
    return outputs


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--with-baritone", action="store_true", default=None, help="build one Sunset package with full Baritone and its engine")
    selection.add_argument("--without-baritone", action="store_false", dest="with_baritone", help="development-only Sunset subset without Baritone runtime")
    args = parser.parse_args()
    try:
        for path in package(with_baritone=args.with_baritone):
            print(path)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging failed: {error}\n")
