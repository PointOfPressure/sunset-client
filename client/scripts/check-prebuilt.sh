#!/bin/bash
# check-prebuilt.sh OUT [PUBKEY]: checks a build-prebuilt.sh output folder: signatures (and that a tampered package,
# an edited manifest and another key are refused), checksums, the package layout, and that no Sunset source ships.
O=$(realpath "$1"); PUB=$(realpath "${2:-$(dirname "$0")/../../launcher/sunset.pub}"); T=$(mktemp -d); pass=0; fail=0
ok(){ if eval "$2" >/dev/null 2>&1; then echo "PASS $1"; pass=$((pass+1)); else echo "FAIL $1"; fail=$((fail+1)); fi; }
no(){ if eval "$2" >/dev/null 2>&1; then echo "FAIL $1"; fail=$((fail+1)); else echo "PASS $1"; pass=$((pass+1)); fi; }
N=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["artifacts"][0]["file"])' $O/manifest.json)
VD=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version_dir"])' $O/manifest.json)
PLAT=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["platform"])' $O/manifest.json)
# Sunset's libraries as rustc names them on the package's platform.
case $PLAT in windows-*) LIBS="horizon_patch.dll sunset_core.dll" ;; *) LIBS="libhorizon_patch.so libsunset_core.so" ;; esac
# pe_imports: fails when a program or library in the folder imports a DLL that ships with the game (Rust's std, the
# MinGW runtime, Horizon's and Sunset's libraries) and isn't beside it: Windows' counterpart of the ldd check.
pe_imports() { python3 - "$1" <<'EOF'
import fnmatch, os, struct, sys
folder = sys.argv[1]
ours = ["std-*.dll", "libgcc*.dll", "libwinpthread*.dll", "libstdc++*.dll", "horizon_*.dll", "mod_*.dll", "sunset_*.dll"]
def imports(path):
    b = open(path, "rb").read()
    pe = struct.unpack_from("<I", b, 0x3C)[0]
    sections, optional = struct.unpack_from("<H", b, pe + 6)[0], struct.unpack_from("<H", b, pe + 20)[0]
    opt = pe + 24
    rva = struct.unpack_from("<I", b, opt + (112 if struct.unpack_from("<H", b, opt)[0] == 0x20B else 96) + 8)[0]
    table = [struct.unpack_from("<III", b, opt + optional + 40 * i + 8) for i in range(sections)]
    def off(r):
        for size, va, raw in ((s[0], s[1], struct.unpack_from("<I", b, opt + optional + 40 * i + 20)[0]) for i, s in enumerate(table)):
            if va <= r < va + max(size, 1):
                return r - va + raw
        raise ValueError(hex(r))
    names, d = [], off(rva) if rva else None
    while d is not None and struct.unpack_from("<5I", b, d) != (0, 0, 0, 0, 0):
        n = off(struct.unpack_from("<I", b, d + 12)[0])
        names.append(b[n:b.index(b"\0", n)].decode())
        d += 20
    return names
present = {f.lower() for f in os.listdir(folder)}
missing = sorted({(f, i) for f in os.listdir(folder) if f.lower().endswith((".exe", ".dll"))
                  for i in imports(os.path.join(folder, f))
                  if i.lower() not in present and any(fnmatch.fnmatch(i.lower(), p) for p in ours)})
for f, i in missing:
    print(f"{f} imports {i}, which is not in the folder")
sys.exit(1 if missing else 0)
EOF
}
ok "manifest signature" "minisign -Vq -p $PUB -m $O/manifest.json"
for f in $O/*.tar.zst; do ok "signature $(basename $f)" "minisign -Vq -p $PUB -m $f"; done
ok "SHA256SUMS" "cd $O && sha256sum -c --quiet SHA256SUMS"
ok "manifest sha256 = file" "python3 -c 'import json,hashlib,os,sys;O=sys.argv[1];m=json.load(open(os.path.join(O,\"manifest.json\")));assert all(hashlib.sha256(open(os.path.join(O,a[\"file\"]),\"rb\").read()).hexdigest()==a[\"sha256\"] and a[\"size\"]>0 for a in m[\"artifacts\"])' $O"
# The launcher's own update (selfupdate.rs), when the release has one: its file, hash and signature, and that it is
# the launcher it says it is.
if python3 -c 'import json,sys;sys.exit(0 if "launcher" in json.load(open(sys.argv[1])) else 1)' $O/manifest.json; then
	LF=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["launcher"]["file"])' $O/manifest.json)
	LV=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["launcher"]["version"])' $O/manifest.json)
	ok "launcher entry sha256 and size = $LF" "python3 -c 'import json,hashlib,os,sys;O=sys.argv[1];l=json.load(open(os.path.join(O,\"manifest.json\")))[\"launcher\"];p=os.path.join(O,l[\"file\"]);assert hashlib.sha256(open(p,\"rb\").read()).hexdigest()==l[\"sha256\"] and os.path.getsize(p)==l[\"size\"] and l[\"signature\"]==l[\"file\"]+\".minisig\"' $O"
	ok "launcher signature" "minisign -Vq -p $PUB -m $O/$LF"
	[ "$PLAT" != linux-x86_64 ] || ok "launcher says it is $LV" "tar xzf $O/$LF -C $T sunset-launcher/sunset-launcher && [ \"\$(SUNSET_NO_SELF_UPDATE=1 $T/sunset-launcher/sunset-launcher --version)\" = 'sunset-launcher $LV' ]"
fi
cp $O/$N $T/x.tar.zst; cp $O/$N.minisig $T/x.tar.zst.minisig
printf '\x00' | dd of=$T/x.tar.zst bs=1 seek=4096 conv=notrunc 2>/dev/null
no "tampered package rejected" "minisign -Vq -p $PUB -m $T/x.tar.zst"
sed 's/"version": "[^"]*"/"version": "9.9.9"/' $O/manifest.json > $T/m.json; cp $O/manifest.json.minisig $T/m.json.minisig
no "edited manifest rejected" "minisign -Vq -p $PUB -m $T/m.json"
minisign -G -W -p $T/other.pub -s $T/other.key >/dev/null 2>&1
no "other key rejected" "minisign -Vq -p $T/other.pub -m $O/manifest.json"
mkdir $T/x; tar --zstd -xf $O/$N -C $T/x
ok "one top folder = version_dir" "[ \"\$(ls $T/x)\" = $VD ]"
ok "files.sha256" "cd $T/x/$VD && sha256sum -c --quiet files.sha256"
ok "no Sunset source (only upstream horizon-prepare .rs)" "[ -z \"\$(find $T/x -name '*.rs' ! -path '*/crates/horizon-prepare/*')\" ]"
ok "exported package: manifest, Cargo.toml, empty src/" "P=$T/x/$VD/horizon-native-mods/sunset; [ -f \$P/horizon-mod.toml ] && [ -f \$P/Cargo.toml ] && [ -d \$P/src ] && [ -z \"\$(ls -A \$P/src)\" ]"
ok "no Mojang-prepared files (.local)" "[ ! -e $T/x/$VD/.local ]"
if [ "$PLAT" = windows-x86_64 ]; then
	ok "game finds its libraries beside it (PE imports)" "pe_imports $T/x/$VD"
else
	ok "stub needs only libs inside" "cd $T/x/$VD && ! LD_LIBRARY_PATH= ldd ./horizon-client | grep -q 'not found'"
fi
ok "both Sunset libraries inside" "for l in $LIBS; do [ -f $T/x/$VD/\$l ] || exit 1; done"
ok "native-mods.json lists sunset" "grep -q '\"id\":\"sunset\"' $T/x/$VD/native-mods.json"
LICS="HORIZON-NOTICE.txt LICENSE-LGPL-3.0.txt LICENSE-GPL-3.0.txt HORIZON-MIT.txt PREBUILT-TERMS.md SUNSET-LICENSE.txt THIRD-PARTY-LICENSES.txt"
ok "licences/ in the game folder" "for f in $LICS; do [ -s $T/x/$VD/licences/\$f ] || exit 1; done"
ok "Horizon notice names the fork commit" "grep -Eq 'PointOfPressure/horizon/tree/[0-9a-f]{40}' $T/x/$VD/licences/HORIZON-NOTICE.txt"
ok "licences, Horizon source and bundle beside the release" "cd $O && ls LICENSE-LGPL-3.0.txt LICENSE-GPL-3.0.txt PREBUILT-TERMS.md *-HORIZON-NOTICE.txt *-THIRD-PARTY-LICENSES.txt sunset-launcher-THIRD-PARTY-LICENSES.txt *-horizon-source.tar.zst *-horizon-series.bundle"
ok "owner root, no setuid" "[ -z \"\$(tar --zstd -tvf $O/$N | grep -v ' 0/0 ' )\" ] && [ -z \"\$(find $T/x -perm /6000)\" ]"
echo "$pass passed, $fail failed"; rm -rf "$T"; [ $fail = 0 ]
