#!/bin/sh
# relink.sh: links libhorizon_patch.so again from this kit's objects, against the Horizon SDK you give it, and
# installs it into a Sunset game folder. README.md says when that works and what to do when it doesn't.
#
#   ./relink.sh SDK GAME [RUSTC]
#     SDK    the Horizon SDK the game's libraries come from: a folder with deps/ and host/ (the stock
#            launcher's native/prebuilt-package after a native-build, or your own SDK build)
#     GAME   the Sunset game folder (versions/native-...) to install the result into; "-" only links it (into ./out)
#     RUSTC  rustc of exactly the version in RUST below (default: rustc on PATH)
set -eu
KIT=$(cd "$(dirname "$0")" && pwd)
RUST=$(cat "$KIT/RUST")
die() { echo "relink.sh: $*" >&2; exit 1; }
[ $# -ge 2 ] || die "usage: relink.sh SDK GAME|- [RUSTC]"
SDK=$(cd "$1" && pwd); GAME=$2; RUSTC=${3:-rustc}
for d in deps host; do [ -d "$SDK/$d" ] || die "$SDK has no $d/: not an SDK folder"; done
have=$("$RUSTC" --version | cut -d' ' -f2)
[ "$have" = "$RUST" ] || die "$RUSTC is $have; these objects need rustc $RUST (Rust libraries only link with the compiler that made them)"
mkdir -p "$KIT/out"
# The launcher's own link line for the patch library (horizon-launcher 0.2.78, native_mods/prebuilt.rs), with the
# SDK and kit paths put back in.
sed -e "s|\$SDK|$SDK|g" -e "s|\$KIT|$KIT|g" "$KIT/horizon_patch.link" > "$KIT/out/horizon_patch.args"
"$RUSTC" --edition 2024 -C prefer-dynamic -C opt-level=3 -C 'link-arg=-Wl,-rpath,$ORIGIN' \
	-L "dependency=$SDK/deps" -L "dependency=$SDK/host" -L "dependency=$KIT" -L "native=$SDK/native" \
	--crate-type dylib --crate-name horizon_patch "@$KIT/out/horizon_patch.args" \
	-o "$KIT/out/libhorizon_patch.so" "$KIT/horizon_patch.rs"
echo "relink.sh: linked $KIT/out/libhorizon_patch.so"
[ "$GAME" = - ] && exit 0
[ -f "$GAME/native-mods.json" ] && [ -f "$GAME/libhorizon_patch.so" ] || die "$GAME is not a Sunset game folder"
[ -e "$GAME/libhorizon_patch.so.orig" ] || cp -p "$GAME/libhorizon_patch.so" "$GAME/libhorizon_patch.so.orig"
rm -f "$GAME/libhorizon_patch.so"   # a fresh file, never written through a hard link into another copy
cp "$KIT/out/libhorizon_patch.so" "$GAME/libhorizon_patch.so"
# The game checks every mod library against native-mods.json's sha256 before it loads it; files.sha256 is the
# launcher's install record.
python3 - "$GAME" <<'EOF'
import hashlib, json, os, sys
game = sys.argv[1]
h = hashlib.sha256(open(os.path.join(game, "libhorizon_patch.so"), "rb").read()).hexdigest()
p = os.path.join(game, "native-mods.json")
m = json.load(open(p))
for lib in m["libraries"]:
    if lib["file"] == "libhorizon_patch.so":
        lib["sha256"] = h
json.dump(m, open(p, "w"), indent=2)
f = os.path.join(game, "files.sha256")
if os.path.exists(f):
    lines = [f"{h}  libhorizon_patch.so" if l.endswith("  libhorizon_patch.so") else l for l in open(f).read().splitlines()]
    open(f, "w").write("\n".join(lines) + "\n")
EOF
echo "relink.sh: installed into $GAME (the original is libhorizon_patch.so.orig)"
