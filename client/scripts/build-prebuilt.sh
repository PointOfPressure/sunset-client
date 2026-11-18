#!/bin/bash
# build-prebuilt.sh: Sunset's prebuilt client for Linux x86_64, from source on this machine.
#
#   pin (client/sunset/horizon-mod.toml [native]) -> patched source (the stock horizon-launcher's native-build weaves
#   the .hmod into the pinned Horizon's function-replacement crates and compiles them, optimized, against that
#   Nightly's published SDK) -> package (the game folder players run, without Sunset's source) -> manifest.json ->
#   minisign signatures. Nothing is uploaded: publishing the files in $OUT is a manual step.
#
# Environment (all optional):
#   OUT        where the release files go (default: <repo>/dist/prebuilt)
#   WORK       the build's own launcher home, kept for warm rebuilds (default /var/tmp/sunset-prebuilt). rustc embeds
#              this path in the libraries: keep it fixed and outside home folders (no user names, reproducible)
#   LAUNCHER_VERSION  the stock horizon-launcher release used to build (default 0.2.78)
#   MINECRAFT  a Minecraft folder with 26.3 for `prepare` (default: the official launcher's ~/.minecraft)
#   KEY        minisign secret key (default ~/.config/sunset/minisign.key); SIGN=0 skips signing
#   FORK       a horizon-rust/horizon checkout: writes local branch sunset/<pin> (the LGPL corresponding source,
#              never pushed) and exports its readable patches into client/horizon-patches/<pin>/
#   RELINK=0   skips the relink kit (client/horizon-patches/README.md, "LGPL")
set -euo pipefail

REPO=$(cd "$(dirname "$0")/../.." && pwd)
MOD=$REPO/client/sunset
OUT=${OUT:-$REPO/dist/prebuilt}
WORK=${WORK:-/var/tmp/sunset-prebuilt}
LAUNCHER_VERSION=${LAUNCHER_VERSION:-0.2.78}
MINECRAFT=${MINECRAFT:-$HOME/.minecraft}
KEY=${KEY:-$HOME/.config/sunset/minisign.key}
PLATFORM=linux-x86_64
REAL_HOME=$HOME

say() { echo "build-prebuilt: $*" >&2; }
die() { say "$*"; exit 1; }
for tool in curl git python3 sha256sum tar zstd rsync; do command -v $tool >/dev/null || die "needs $tool"; done
[ "${SIGN:-1}" = 0 ] || { command -v minisign >/dev/null || die "needs minisign (SIGN=0 skips signing)"; [ -f "$KEY" ] || die "no minisign key at $KEY"; }

# ---- pin
toml() { python3 -c 'import sys,tomllib;d=tomllib.load(open(sys.argv[1],"rb"))
for k in sys.argv[2].split("."): d=d[k]
print(d)' "$MOD/horizon-mod.toml" "$1"; }
REV=$(toml native.horizon); MIXINS=$(toml native.mixins); GAME=$(toml native.game); VERSION=$(toml version)
SHORT=${REV:0:7}
SRC_COMMIT=$(git -C "$REPO" rev-parse HEAD)
DIRTY=false; [ -z "$(git -C "$REPO" status --porcelain -- client/sunset)" ] || DIRTY=true
say "Sunset $VERSION on Horizon $SHORT (mixins ${MIXINS:0:7}, game $GAME), sunset-client ${SRC_COMMIT:0:7}$($DIRTY && echo ' + local changes')"

# ---- the stock launcher, checked against its release's SHA256SUMS, in its own home under $WORK
mkdir -p "$WORK/launcher-$LAUNCHER_VERSION" "$WORK/home" "$OUT"
L=$WORK/launcher-$LAUNCHER_VERSION/horizon-launcher
if [ ! -x "$L" ]; then
	url=https://github.com/horizon-rust/horizon/releases/download/launcher@$LAUNCHER_VERSION
	tmp=$(mktemp -d)
	curl -fsSL -o "$tmp/horizon-launcher-$PLATFORM.tar.gz" "$url/horizon-launcher-$PLATFORM.tar.gz"
	curl -fsSL -o "$tmp/SHA256SUMS" "$url/SHA256SUMS"
	(cd "$tmp" && grep " horizon-launcher-$PLATFORM.tar.gz\$" SHA256SUMS | sha256sum -c --quiet -) || die "launcher checksum mismatch"
	tar xzf "$tmp/horizon-launcher-$PLATFORM.tar.gz" -C "$WORK/launcher-$LAUNCHER_VERSION"
	rm -rf "$tmp"
fi
hl() { HOME="$WORK/home" HORIZON_LAUNCHER_NO_SELF_UPDATE=1 "$L" "$@"; }
LDATA=$WORK/home/.local/share/horizon-launcher

TAG=$(hl --json versions 2>/dev/null | python3 -c 'import json,sys
t=[v["tag"] for v in json.load(sys.stdin) if v["tag"].startswith("nightly-") and v["tag"].endswith("-"+sys.argv[1])]
print(t[0] if t else "")' "$SHORT")
[ -n "$TAG" ] || die "no Nightly release for $SHORT (pin a Nightly that still has its SDK)"

if [ ! -e "$LDATA/versions/$TAG/.installed" ]; then
	say "installing $TAG"; hl install "$TAG" >/dev/null
fi
if [ ! -d "$LDATA/versions/$TAG/.local/horizon-client" ]; then
	[ -d "$MINECRAFT/versions/$GAME" ] || die "prepare needs Minecraft $GAME: set MINECRAFT to a folder with versions/$GAME"
	hl settings minecraft-dir "$MINECRAFT" >/dev/null
	say "preparing $TAG from $MINECRAFT"; hl prepare --minecraft "$MINECRAFT" --version "$TAG" >/dev/null
fi
hl rust status >/dev/null 2>&1 || { say "installing the launcher's Rust"; hl rust install >/dev/null; }
INST=sunset-prebuilt
hl instance create $INST >/dev/null 2>&1 || true
hl instance version $INST "$TAG" >/dev/null

# ---- patched source + release build (the launcher's optimized profile)
STAGE=$WORK/stage
rm -rf "$STAGE"; mkdir -p "$STAGE/sunset"
rsync -a --exclude /.horizon --exclude /target "$MOD/" "$STAGE/sunset/"
say "native-build (launcher $LAUNCHER_VERSION)"
start=$(date +%s)
hl native-build "$STAGE" --instance $INST --trust-native-code > "$WORK/native-build.log" 2>&1 || { tail -20 "$WORK/native-build.log" >&2; die "native-build failed ($WORK/native-build.log)"; }
BUILD_SECS=$(( $(date +%s) - start ))
VDIR=$(grep -o 'native-[0-9a-f]*-[0-9a-f]* built' "$WORK/native-build.log" | tail -1 | cut -d' ' -f1)
[ -n "$VDIR" ] || die "native-build printed no version"
V=$LDATA/versions/$VDIR
WARN=$(grep -A1 '^warning:' "$LDATA/native/build.log" | grep -c -- '-->' || true)
say "built $VDIR in $BUILD_SECS s ($WARN rustc warnings)"
BUILD=$(ls -dt "$LDATA"/native/prebuilt-build/rustc-"${REV:0:12}"-* | head -1)
[ -d "$BUILD/patch-applied" ] || die "no woven patch workspace in $BUILD"
RUST=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["rust"])' "$V/native-mods.json")

# ---- package: the version folder as the launcher installs it, minus Horizon's developer tools, anything prepared
# from Mojang's files (.local) and Sunset's Rust source. The game checks each exported package at start
# (horizon-mod-package runtime.rs: horizon-mod.toml's id, version and side, and that Cargo.toml and src/ are both
# there), so horizon-native-mods/<id>/ keeps its manifest, Cargo.toml and assets with an empty src/.
NAME=sunset-$VERSION-$SHORT-$PLATFORM
PKG=$WORK/pkg/$VDIR
rm -rf "$WORK/pkg"; mkdir -p "$PKG"
rsync -a --exclude /.local --exclude /tools --exclude '/horizon-native-mods/*/src/*' "$V/" "$PKG/"
[ -z "$(find "$PKG/horizon-native-mods" -name '*.rs')" ] || die "Rust source left in the package"
(cd "$PKG" && find . -type f ! -name files.sha256 -printf '%P\n' | LC_ALL=C sort | xargs -d '\n' sha256sum > files.sha256)
UNPACKED=$(du -sb "$PKG" | cut -f1)
tar -C "$WORK/pkg" --owner=0 --group=0 --numeric-owner --mtime=@"$(git -C "$REPO" log -1 --format=%ct)" --sort=name \
	-cf - "$VDIR" | zstd -q -19 -T0 -f -o "$OUT/$NAME.tar.zst"

# ---- relink kit: the objects of Sunset's two libraries and their link inputs (see horizon-patches/README.md)
if [ "${RELINK:-1}" != 0 ]; then
	KIT=$WORK/relink/$NAME-relink; rm -rf "$WORK/relink"; mkdir -p "$KIT"
	cp "$BUILD"/*.rlib "$BUILD"/*.externs "$BUILD"/*.identity.json "$BUILD"/libhorizon_patch.so "$BUILD"/libsunset_core.so "$BUILD"/horizon_patch.rs "$KIT/"
	sed -i "s|$LDATA/native/prebuilt-package|\$SDK|g; s|$BUILD|\$KIT|g" "$KIT"/*.externs
	cp "$REPO/client/horizon-patches/README.md" "$KIT/README.md"
	tar -C "$WORK/relink" --owner=0 --group=0 --numeric-owner --sort=name -cf - "$NAME-relink" | zstd -q -19 -T0 -f -o "$OUT/$NAME-relink.tar.zst"
fi

# ---- the LGPL corresponding source as a patch series on the pin (local branch; pushing is a manual step)
FORK_HEAD=
if [ -n "${FORK:-}" ]; then
	BR=sunset/$SHORT
	git -C "$FORK" cat-file -e "$REV^{commit}" 2>/dev/null || git -C "$FORK" fetch -q origin "$REV"
	FW=$WORK/fork-tree; rm -rf "$FW"
	git -C "$FORK" worktree prune
	git -C "$FORK" worktree add -q -f -B "$BR" "$FW" "$REV"
	export GIT_AUTHOR_DATE="@$(git -C "$REPO" log -1 --format=%ct)" GIT_COMMITTER_DATE="@$(git -C "$REPO" log -1 --format=%ct)"
	gc() { git -C "$FW" add -A sunset && git -C "$FW" commit -q -m "$1" -m "$2"; }
	mkdir -p "$FW/sunset/package" "$FW/sunset/patch"
	rsync -a "$STAGE/sunset/" "$FW/sunset/package/"
	cp "$REPO/LICENSE" "$FW/sunset/package/LICENSE"
	gc "Sunset $VERSION: the .hmod package" "client/sunset of PointOfPressure/sunset-client $SRC_COMMIT$($DIRTY && echo ' (with local changes)'), under its own licence (sunset/package/LICENSE), not Horizon's."
	rsync -a --exclude /.horizon-mixins-stage.json --exclude /native-post-build.json --exclude /target "$BUILD/patch-workspace/" "$FW/sunset/patch/"
	gc "Function-replacement crates of $SHORT, as generated" "What horizon-launcher $LAUNCHER_VERSION native-build extracts from this revision's horizon-client, horizon-client-lib, -render and -input for the replaced functions, before any mod is applied. Unmodified Horizon code (LGPL-3.0-or-later)."
	rsync -a --exclude /target "$BUILD/patch-applied/" "$FW/sunset/patch/"
	cp "$BUILD/horizon_patch.rs" "$FW/sunset/patch/horizon_patch.rs"
	gc "Sunset's mixins woven in (horizon-mixins ${MIXINS:0:7})" "The source of libhorizon_patch.so as shipped: sunset/package/horizon-mod.toml's mixins and support modules applied by horizon-mixins, and the launcher's entry module (horizon_patch.rs). Rust $RUST."
	sed -e "s/@VERSION@/$VERSION/g; s/@REV@/$REV/g; s/@TAG@/$TAG/g; s/@LAUNCHER@/$LAUNCHER_VERSION/g; s/@MIXINS@/$MIXINS/g; s/@RUST@/$RUST/g; s/@SRC@/$SRC_COMMIT/g" \
		"$REPO/client/horizon-patches/fork-README.md" > "$FW/sunset/README.md"
	gc "Sunset: how this branch is built and licensed" "sunset/README.md."
	FORK_HEAD=$(git -C "$FW" rev-parse HEAD)
	P=$REPO/client/horizon-patches/$SHORT
	rm -rf "$P"; mkdir -p "$P"
	# The weave and the notes are the readable part; the package is client/sunset itself and the extract is generated.
	git -C "$FW" format-patch -q --no-signature -o "$P" "$BR~2..$BR"
	git -C "$FW" log --reverse --format='%H %s' "$REV..$BR" > "$P/series"
	git -C "$FORK" worktree remove --force "$FW"
	say "fork branch $BR = ${FORK_HEAD:0:10} (local, not pushed); patches in client/horizon-patches/$SHORT"
fi

# ---- manifest + signatures
# The Sunset launcher downloads the same stock CLI from upstream and checks it against this signed hash.
case $PLATFORM in windows-x86_64) CLI_FILE=horizon-launcher-windows-x64.zip ;; *) CLI_FILE=horizon-launcher-$PLATFORM.tar.gz ;; esac
CLI_URL=https://github.com/horizon-rust/horizon/releases/download/launcher@$LAUNCHER_VERSION/$CLI_FILE
CLI_SHA=$(curl -fsSL "https://github.com/horizon-rust/horizon/releases/download/launcher@$LAUNCHER_VERSION/SHA256SUMS" | awk -v f="$CLI_FILE" '$2 == f || $2 == "*" f {print $1}')
[ ${#CLI_SHA} = 64 ] || die "no SHA-256 for $CLI_FILE in launcher@$LAUNCHER_VERSION's SHA256SUMS"
python3 - "$OUT" "$NAME" "$VDIR" "$UNPACKED" <<EOF
import hashlib, json, os, sys, datetime
out, name, vdir, unpacked = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
def art(f, kind):
    p = os.path.join(out, f)
    h = hashlib.sha256(open(p, "rb").read()).hexdigest()
    return {"file": f, "kind": kind, "size": os.path.getsize(p), "sha256": h, "signature": f + ".minisig"}
arts = [art(name + ".tar.zst", "game")]
arts[0]["unpacked_size"] = unpacked
if os.path.exists(os.path.join(out, name + "-relink.tar.zst")):
    arts.append(art(name + "-relink.tar.zst", "relink"))
m = {
    "schema": 1,
    "id": "sunset",
    "version": "$VERSION",
    "platform": "$PLATFORM",
    "version_dir": vdir,
    "horizon": {"revision": "$REV", "tag": "$TAG", "game": "$GAME", "mixins": "$MIXINS", "sdk": "sdk-$REV"},
    "build": {"launcher": "$LAUNCHER_VERSION", "rust": "$RUST", "profile": "optimized", "seconds": $BUILD_SECS,
              "built": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")},
    "cli": {"version": "$LAUNCHER_VERSION", "file": "$CLI_FILE", "url": "$CLI_URL", "sha256": "$CLI_SHA"},
    "source": {"sunset_client": "$SRC_COMMIT", "local_changes": "$DIRTY" == "true",
               "horizon_fork": "https://github.com/PointOfPressure/horizon/tree/sunset/$SHORT",
               "horizon_fork_commit": "$FORK_HEAD" or None},
    "artifacts": arts,
}
json.dump(m, open(os.path.join(out, "manifest.json"), "w"), indent=2)
open(os.path.join(out, "manifest.json"), "a").write("\n")
EOF
(cd "$OUT" && sha256sum manifest.json "$NAME".tar.zst $(ls "$NAME"-relink.tar.zst 2>/dev/null) > SHA256SUMS)
if [ "${SIGN:-1}" != 0 ]; then
	for f in "$NAME.tar.zst" $(cd "$OUT" && ls "$NAME"-relink.tar.zst 2>/dev/null) manifest.json; do
		minisign -S -s "$KEY" -m "$OUT/$f" -t "sunset $VERSION $SHORT $PLATFORM $f" >/dev/null </dev/null
	done
	minisign -V -q -p "$REPO/launcher/sunset.pub" -m "$OUT/manifest.json" || die "signature check failed"
fi
say "done: $OUT ($(du -h "$OUT/$NAME.tar.zst" | cut -f1) package, $(( UNPACKED / 1048576 )) MiB unpacked)"
