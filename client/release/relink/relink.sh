#!/bin/sh
# relink.sh: links libhorizon_patch.so again from this kit's objects, against the Horizon SDK you give it, and
# installs it into a Sunset game folder. README.md says when that works and what to do when it doesn't.
#
#   ./relink.sh SDK GAME [RUSTC]
#     SDK    the Horizon SDK the game's libraries come from: a folder with deps/ and host/ (the stock
#            launcher's native/prebuilt-package after a native-build, or your own SDK build)
#     GAME   the Sunset game folder (versions/native-...) to install the result into; "-" only links it (into ./out)
#     RUSTC  rustc of exactly the version in RUST below (default: rustc on PATH)
# On Windows it runs in Git Bash, with the toolchain's MinGW (the launcher's rust/<version>/mingw64/bin) on PATH.
set -eu
PLATFORM=$(cat "$(dirname "$0")/PLATFORM" 2>/dev/null || echo linux-x86_64)
# rustc gets the paths in an @file, which no shell rewrites: Windows spelling (C:/...) there.
case $PLATFORM in windows-*) here() { pwd -W; }; LIB=horizon_patch.dll ;; *) here() { pwd; }; LIB=libhorizon_patch.so ;; esac
KIT=$(cd "$(dirname "$0")" && here)
RUST=$(cat "$KIT/RUST")
die() { echo "relink.sh: $*" >&2; exit 1; }
[ $# -ge 2 ] || die "usage: relink.sh SDK GAME|- [RUSTC]"
SDK=$(cd "$1" && here); GAME=$2; RUSTC=${3:-rustc}
# Windows' python3 is often the Microsoft Store's stub.
PY=python3; $PY -c '' 2>/dev/null || PY=python
for d in deps host; do [ -d "$SDK/$d" ] || die "$SDK has no $d/: not an SDK folder"; done
have=$("$RUSTC" --version | cut -d' ' -f2)
[ "$have" = "$RUST" ] || die "$RUSTC is $have; these objects need rustc $RUST (Rust libraries only link with the compiler that made them)"
mkdir -p "$KIT/out"
# The launcher's own link line for the patch library (horizon-launcher 0.2.78, native_mods/prebuilt.rs), with the
# SDK and kit paths put back in.
sed -e "s|\$SDK|$SDK|g" -e "s|\$KIT|$KIT|g" "$KIT/horizon_patch.link" > "$KIT/out/horizon_patch.args"
# Linux: the library finds Horizon's beside it ($ORIGIN); Windows looks beside the game's executable by itself.
case $PLATFORM in windows-*) set -- ;; *) set -- -C 'link-arg=-Wl,-rpath,$ORIGIN' ;; esac
"$RUSTC" --edition 2024 -C prefer-dynamic -C opt-level=3 "$@" \
	-L "dependency=$SDK/deps" -L "dependency=$SDK/host" -L "dependency=$KIT" -L "native=$SDK/native" \
	--crate-type dylib --crate-name horizon_patch "@$KIT/out/horizon_patch.args" \
	-o "$KIT/out/$LIB" "$KIT/horizon_patch.rs"
echo "relink.sh: linked $KIT/out/$LIB"
[ "$GAME" = - ] && exit 0
[ -f "$GAME/native-mods.json" ] && [ -f "$GAME/$LIB" ] || die "$GAME is not a Sunset game folder"
[ -e "$GAME/$LIB.orig" ] || cp -p "$GAME/$LIB" "$GAME/$LIB.orig"
rm -f "$GAME/$LIB"   # a fresh file, never written through a hard link into another copy
cp "$KIT/out/$LIB" "$GAME/$LIB"
# The game checks every mod library against native-mods.json's sha256 before it loads it; files.sha256 is the
# launcher's install record.
$PY - "$GAME" "$LIB" <<'EOF'
import hashlib, json, os, sys
game, lib_name = sys.argv[1], sys.argv[2]
h = hashlib.sha256(open(os.path.join(game, lib_name), "rb").read()).hexdigest()
p = os.path.join(game, "native-mods.json")
m = json.load(open(p))
for lib in m["libraries"]:
    if lib["file"] == lib_name:
        lib["sha256"] = h
json.dump(m, open(p, "w", newline="\n"), indent=2)
f = os.path.join(game, "files.sha256")
if os.path.exists(f):
    lines = [f"{h}  {lib_name}" if l.endswith("  " + lib_name) else l for l in open(f).read().splitlines()]
    open(f, "w", newline="\n").write("\n".join(lines) + "\n")
EOF
echo "relink.sh: installed into $GAME (the original is $LIB.orig)"
