#!/bin/bash
# check-prebuilt.sh OUT [PUBKEY]: checks a build-prebuilt.sh output folder: signatures (and that a tampered package,
# an edited manifest and another key are refused), checksums, the package layout, and that no Sunset source ships.
O=$(realpath "$1"); PUB=$(realpath "${2:-$(dirname "$0")/../../launcher/sunset.pub}"); T=$(mktemp -d); pass=0; fail=0
ok(){ if eval "$2" >/dev/null 2>&1; then echo "PASS $1"; pass=$((pass+1)); else echo "FAIL $1"; fail=$((fail+1)); fi; }
no(){ if eval "$2" >/dev/null 2>&1; then echo "FAIL $1"; fail=$((fail+1)); else echo "PASS $1"; pass=$((pass+1)); fi; }
N=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["artifacts"][0]["file"])' $O/manifest.json)
VD=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version_dir"])' $O/manifest.json)
ok "manifest signature" "minisign -Vq -p $PUB -m $O/manifest.json"
for f in $O/*.tar.zst; do ok "signature $(basename $f)" "minisign -Vq -p $PUB -m $f"; done
ok "SHA256SUMS" "cd $O && sha256sum -c --quiet SHA256SUMS"
ok "manifest sha256 = file" "python3 -c 'import json,hashlib,sys;m=json.load(open(\"$O/manifest.json\"));assert all(hashlib.sha256(open(\"$O/\"+a[\"file\"],\"rb\").read()).hexdigest()==a[\"sha256\"] and a[\"size\"]>0 for a in m[\"artifacts\"])'"
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
ok "stub needs only libs inside" "cd $T/x/$VD && ! LD_LIBRARY_PATH= ldd ./horizon-client | grep -q 'not found'"
ok "both Sunset libraries inside" "[ -f $T/x/$VD/libhorizon_patch.so ] && [ -f $T/x/$VD/libsunset_core.so ]"
ok "native-mods.json lists sunset" "grep -q '\"id\":\"sunset\"' $T/x/$VD/native-mods.json"
LICS="HORIZON-NOTICE.txt LICENSE-LGPL-3.0.txt LICENSE-GPL-3.0.txt HORIZON-MIT.txt PREBUILT-TERMS.md SUNSET-LICENSE.txt THIRD-PARTY-LICENSES.txt"
ok "licences/ in the game folder" "for f in $LICS; do [ -s $T/x/$VD/licences/\$f ] || exit 1; done"
ok "Horizon notice names the fork commit" "grep -Eq 'PointOfPressure/horizon/tree/[0-9a-f]{40}' $T/x/$VD/licences/HORIZON-NOTICE.txt"
ok "licences, Horizon source and bundle beside the release" "cd $O && ls LICENSE-LGPL-3.0.txt LICENSE-GPL-3.0.txt PREBUILT-TERMS.md *-HORIZON-NOTICE.txt *-THIRD-PARTY-LICENSES.txt sunset-launcher-THIRD-PARTY-LICENSES.txt *-horizon-source.tar.zst *-horizon-series.bundle"
ok "owner root, no setuid" "[ -z \"\$(tar --zstd -tvf $O/$N | grep -v ' 0/0 ' )\" ] && [ -z \"\$(find $T/x -perm /6000)\" ]"
echo "$pass passed, $fail failed"; rm -rf "$T"; [ $fail = 0 ]
