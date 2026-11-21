#!/bin/bash
# build-prebuilt.sh: Sunset's prebuilt client for Linux x86_64 or Windows x64, from source on this machine.
#
#   pin (client/sunset/horizon-mod.toml [native]) -> patched source (the stock horizon-launcher's native-build weaves
#   the .hmod into the pinned Horizon's function-replacement crates and compiles them, optimized, against that
#   Nightly's published SDK) -> package (the game folder players run, without Sunset's source) -> manifest.json ->
#   minisign signatures. Nothing is uploaded: publishing the files in $OUT is a manual step.
#
# Environment (all optional):
#   OUT        where the release files go (default: <repo>/dist/prebuilt)
#   PLATFORM   linux-x86_64 or windows-x86_64 (default: this machine's). Each is built on its own OS: Windows in Git
#              Bash with python3 (or python), zstd and unzip; its libraries are .dll, its launcher the .zip
#   WORK       the build's own launcher home, kept for warm rebuilds (default /var/tmp/sunset-prebuilt, on Windows
#              C:/sunset-prebuilt). rustc embeds this path in the libraries: keep it fixed and outside home folders
#              (no user names, reproducible)
#   LAUNCHER_VERSION  the stock horizon-launcher release used to build (default 0.2.78)
#   MINECRAFT  a Minecraft folder with 26.3 for `prepare` (default: the official launcher's ~/.minecraft, on Windows
#              %APPDATA%/.minecraft)
#   KEY        minisign secret key (default ~/.config/sunset/minisign.key); SIGN=0 skips signing
#   FORK       a horizon-rust/horizon checkout (required): writes local branch sunset/<pin> (the LGPL corresponding
#              source, never pushed), exports its readable patches into client/horizon-patches/<pin>/, and gives the
#              release Horizon's source (tarball + git bundle) and the third-party licences of its dependency tree
#   RELINK=0   skips the relink kit (client/horizon-patches/README.md, "LGPL")
#   BASE, NIGHTLY  a Horizon game folder of the pin (`cargo run -p xtask -- package`, what a Nightly's archive holds)
#              installed as Nightly NIGHTLY (nightly-<date>-<pin>) when the launcher can no longer download that
#              Nightly. Needs HOOKS (the fork SDK stands in for the Nightly's signed SDK index)
#   HOOKS      with FORK: the commit or branch holding Sunset's hook patches on the pin (sunset-hooks/<pin>). The build
#              then uses the fork's SDK (client/scripts/fork-sdk.sh build, in SDK, default ~/sunset-ops/tmp/fork-sdk),
#              which must be of exactly that commit, and the fork branch starts from it
#
# Besides the game package, the release carries the launcher's plugin kit: `…-source.tar.zst` (Sunset's .hmod package
# as built, one sunset/ folder: the launcher builds installed plugins into it) and, with HOOKS, `sunset-sdk-<pin>-<fork
# commit>-<platform>.tar.zst` (that SDK and the fork's source, one fork-sdk/ folder), both signed like the package.
# Licences (client/release/): the game folder gets licences/ (Horizon notice, LGPL and GPL texts, PREBUILT-TERMS.md,
# Sunset's licence, THIRD-PARTY-LICENSES.txt from client/release/third-party.sh, which needs cargo-about), and the
# same files, Horizon's source and the launcher's THIRD-PARTY-LICENSES.txt go beside the release, signed.
set -euo pipefail

REPO=$(cd "$(dirname "$0")/../.." && pwd)
MOD=$REPO/client/sunset
say() { echo "build-prebuilt: $*" >&2; }
die() { say "$*"; exit 1; }
case $(uname -s) in MINGW*|MSYS*|CYGWIN*) HOST=windows-x86_64 ;; *) HOST=linux-x86_64 ;; esac
PLATFORM=${PLATFORM:-$HOST}
case $PLATFORM in linux-x86_64|windows-x86_64) ;; *) die "PLATFORM is linux-x86_64 or windows-x86_64, not $PLATFORM" ;; esac
# The launcher's native-build compiles with its own Rust for the OS it runs on: no cross builds.
[ "$PLATFORM" = "$HOST" ] || die "a $PLATFORM build runs on $PLATFORM (this is $HOST)"
OUT=${OUT:-$REPO/dist/prebuilt}
LAUNCHER_VERSION=${LAUNCHER_VERSION:-0.2.78}
KEY=${KEY:-$HOME/.config/sunset/minisign.key}
REAL_HOME=$HOME
if [ "$PLATFORM" = windows-x86_64 ]; then
	WORK=${WORK:-C:/sunset-prebuilt}
	MINECRAFT=${MINECRAFT:-$APPDATA/.minecraft}
	# One spelling of every path (/c/...): GNU tar reads C:/... as a remote host, and the sed rewrites below match it.
	for v in OUT WORK MINECRAFT KEY ${SDK:+SDK} ${FORK:+FORK} ${BASE:+BASE}; do printf -v "$v" %s "$(cygpath -u "${!v}")"; done
	LAUNCHER_FILE=horizon-launcher-windows-x64.zip; LAUNCHER_EXE=horizon-launcher.exe
	DLL_PREFIX= DLL=.dll
	# The commits' bytes in every checkout this script makes (Git for Windows defaults to core.autocrlf=true), and
	# UTF-8 for Python's files and pipes instead of the console's code page.
	export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=false PYTHONUTF8=1
	# What the launcher weaves and ships is client/sunset as committed: CRLF would end up in the package and the source.
	[ -z "$(git -C "$REPO" ls-files --eol client/sunset client/release | grep 'w/crlf')" ] ||
		die "this checkout has CRLF line ends (core.autocrlf): clone it with -c core.autocrlf=false"
else
	WORK=${WORK:-/var/tmp/sunset-prebuilt}
	MINECRAFT=${MINECRAFT:-$HOME/.minecraft}
	LAUNCHER_FILE=horizon-launcher-$PLATFORM.tar.gz; LAUNCHER_EXE=horizon-launcher
	DLL_PREFIX=lib DLL=.so
fi
# How the launcher, a native Windows program, gets a path (MSYS rewrites arguments, not every variable).
native_path() { if [ "$PLATFORM" = windows-x86_64 ]; then cygpath -m "$1"; else echo "$1"; fi; }
# Sunset's two libraries as rustc names a dylib on this platform; dllnames gives the texts that name the Linux
# libraries (libhorizon_patch.so, libmod_*.so, ...) this platform's names.
PATCH_LIB=${DLL_PREFIX}horizon_patch$DLL CORE_LIB=${DLL_PREFIX}sunset_core$DLL
export PLATFORM DLL_PREFIX DLL
dllnames() { if [ "$PLATFORM" = windows-x86_64 ]; then sed 's/\blib\([A-Za-z0-9_*-]*\)\.so\b/\1.dll/g'; else cat; fi; }
# sha256sum's own format on both systems (Git for Windows' marks every file binary: "<hash> *<file>").
sums_fix() { sed 's/^\([0-9a-f]\{64\}\) \*/\1  /'; }
sums() { sha256sum "$@" | sums_fix; }
# Windows' python3 is often the Microsoft Store's stub: then python, through a python3 of our own on PATH (the
# helper scripts call python3 too).
if ! python3 -c '' 2>/dev/null; then
	command -v python >/dev/null && python -c 'import sys; assert sys.version_info >= (3, 11)' 2>/dev/null ||
		die "needs python3 (3.11 or newer)"
	mkdir -p "$WORK/bin"; printf '#!/bin/sh\nexec python "$@"\n' > "$WORK/bin/python3"; chmod +x "$WORK/bin/python3"
	export PATH=$WORK/bin:$PATH
fi
tools="curl git python3 sha256sum tar zstd"; [ "$PLATFORM" = windows-x86_64 ] && tools="$tools unzip cygpath"
for tool in $tools; do command -v $tool >/dev/null || die "needs $tool"; done
# copy SRC DST [PATTERN...]: SRC's contents into DST (made if missing), leaving out the paths that match a PATTERN
# (rsync's anchored excludes: /name at the top, /a/*/b below it). rsync where it is (Git for Windows has none).
copy() {
	local src=$1 dst=$2; shift 2
	if command -v rsync >/dev/null; then
		local args=(); for p in "$@"; do args+=(--exclude "$p"); done
		rsync -a ${args[@]+"${args[@]}"} "$src/" "$dst/"
	else
		# Without the leading /: MSYS would rewrite /name into a path under Git's folder before Python sees it.
		python3 - "$src" "$dst" "${@#/}" <<'PY'
import fnmatch, os, shutil, sys
src, dst, patterns = sys.argv[1], sys.argv[2], [p.lstrip("/") for p in sys.argv[3:]]
def skip(rel):
    return any(fnmatch.fnmatchcase(rel, p) and rel.count("/") == p.count("/") for p in patterns)
for root, dirs, files in os.walk(src):
    rel = os.path.relpath(root, src).replace(os.sep, "/")
    rel = "" if rel == "." else rel + "/"
    dirs[:] = [d for d in dirs if not skip(rel + d)]
    os.makedirs(os.path.join(dst, rel), exist_ok=True)
    for f in files:
        if not skip(rel + f):
            shutil.copy2(os.path.join(root, f), os.path.join(dst, rel, f), follow_symlinks=False)
PY
	fi
}
[ -n "${FORK:-}" ] || die "needs FORK=<a horizon-rust/horizon checkout>: the release carries Horizon's source and licences"
git -C "$FORK" var GIT_COMMITTER_IDENT >/dev/null 2>&1 || die "git has no identity in $FORK (user.name, user.email): the fork branch's commits need one"
"$REPO/client/release/third-party.sh" check
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
L=$WORK/launcher-$LAUNCHER_VERSION/$LAUNCHER_EXE
if [ ! -x "$L" ]; then
	url=https://github.com/horizon-rust/horizon/releases/download/launcher@$LAUNCHER_VERSION
	tmp=$(mktemp -d)
	curl -fsSL -o "$tmp/$LAUNCHER_FILE" "$url/$LAUNCHER_FILE"
	curl -fsSL -o "$tmp/SHA256SUMS" "$url/SHA256SUMS"
	(cd "$tmp" && grep " $LAUNCHER_FILE\$" SHA256SUMS | sha256sum -c --quiet -) || die "launcher checksum mismatch"
	case $LAUNCHER_FILE in
	*.zip) unzip -q -o "$tmp/$LAUNCHER_FILE" "$LAUNCHER_EXE" -d "$WORK/launcher-$LAUNCHER_VERSION" ;;
	*) tar xzf "$tmp/$LAUNCHER_FILE" -C "$WORK/launcher-$LAUNCHER_VERSION" ;;
	esac
	rm -rf "$tmp"
fi
LDATA=$WORK/home/.local/share/horizon-launcher
# The launcher's data in $LDATA on both systems (Linux finds it from HOME; Windows' launcher reads %APPDATA% unless
# HORIZON_LAUNCHER_HOME and HORIZON_HOME name the folders), and nothing of it on the real desktop or in its Trash.
if [ "$PLATFORM" = windows-x86_64 ]; then
	mkdir -p "$LDATA/home" "$WORK/trash" "$WORK/clipboard" "$WORK/desktop"
	HORIZON_LAUNCHER_HOME=$(native_path "$LDATA") HORIZON_HOME=$(native_path "$LDATA/home")
	HORIZON_LAUNCHER_TRASH=$(native_path "$WORK/trash") HORIZON_LAUNCHER_CLIPBOARD=$(native_path "$WORK/clipboard")
	HORIZON_LAUNCHER_DESKTOP=$(native_path "$WORK/desktop")
	export HORIZON_LAUNCHER_HOME HORIZON_HOME HORIZON_LAUNCHER_TRASH HORIZON_LAUNCHER_CLIPBOARD HORIZON_LAUNCHER_DESKTOP
fi
hl() { HOME="$WORK/home" HORIZON_LAUNCHER_NO_SELF_UPDATE=1 "$L" "$@"; }

if [ -n "${BASE:-}" ]; then
	# The pin's game folder built here, for a Nightly the launcher can no longer download (Horizon's Nightly is one
	# rolling release now): installed as that Nightly, so the instance, the package's pin and the build id agree.
	TAG=${NIGHTLY:-}
	case $TAG in nightly-????-??-??-"$SHORT") ;; *) die "BASE needs NIGHTLY=nightly-<date>-$SHORT, the Nightly it stands in for" ;; esac
	[ -n "${HOOKS:-}" ] || [ -d "$LDATA/native/sdk/$REV" ] || die "BASE needs HOOKS: the launcher can't fetch $TAG's signed SDK index"
	if [ ! -e "$LDATA/versions/$TAG/.installed" ]; then
		exe=horizon-client; [ "$PLATFORM" = windows-x86_64 ] && exe=horizon-client.exe
		[ -f "$BASE/$exe" ] && [ -f "$BASE/horizon-prepare${exe#horizon-client}" ] && [ -d "$BASE/exporters" ] ||
			die "$BASE is not a packaged Horizon game folder (cargo run -p xtask -- package)"
		say "installing $TAG from $BASE"
		rm -rf "$LDATA/versions/$TAG"; mkdir -p "$LDATA/versions"
		copy "$BASE" "$LDATA/versions/$TAG" /.local /.installed
		touch "$LDATA/versions/$TAG/.installed"
		# The build id its release would name (fork-sdk.sh seed writes the same for the Nightlies it finds).
		mkdir -p "$LDATA/native/sdk/versions"; echo "$REV" > "$LDATA/native/sdk/versions/$TAG"
	fi
fi
[ -n "${TAG:-}" ] || TAG=$(hl --json versions 2>/dev/null | python3 -c 'import json,sys
t=[v["tag"] for v in json.load(sys.stdin) if v["tag"].startswith("nightly-") and v["tag"].endswith("-"+sys.argv[1])]
print(t[0] if t else "")' "$SHORT")
# A Nightly drops off the list after a while: one this home already installed still builds (its SDK is cached).
[ -n "$TAG" ] || TAG=$(cd "$LDATA/versions" 2>/dev/null && ls -d nightly-*-"$SHORT" 2>/dev/null | head -1) || true
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

# ---- the fork's SDK (HOOKS) or the Nightly's published one; a home seeded with the fork's never builds without it
if [ -n "${HOOKS:-}" ]; then
	[ -n "${FORK:-}" ] || die "HOOKS needs FORK"
	SDK=${SDK:-$REAL_HOME/sunset-ops/tmp/fork-sdk}; HOOKS_HEAD=$(git -C "$FORK" rev-parse "$HOOKS^{commit}")
	[ "$(cat "$SDK/fork-commit" 2>/dev/null)" = "$HOOKS_HEAD" ] || die "$SDK is not the SDK of $HOOKS (${HOOKS_HEAD:0:10}): FORK=<worktree at it> client/scripts/fork-sdk.sh build"
	git -C "$FORK" merge-base --is-ancestor "$REV" "$HOOKS_HEAD" || die "$HOOKS is not based on $SHORT"
	[ "$(cat "$LDATA/native/.fork-seeded" 2>/dev/null)" = "$HOOKS_HEAD" ] || SDK=$SDK "$REPO/client/scripts/fork-sdk.sh" seed "$WORK/home"
	hl() { HOME="$WORK/home" HORIZON_LAUNCHER_NO_SELF_UPDATE=1 HORIZON_SDK_BASE="$(native_path "$SDK")" "$L" "$@"; }
	say "Horizon fork SDK of ${HOOKS_HEAD:0:10} (hook patches $(git -C "$FORK" rev-list --count "$REV..$HOOKS_HEAD"))"
elif [ -e "$LDATA/native/.fork-seeded" ]; then
	die "$WORK's launcher home holds the fork SDK: set HOOKS, or use another WORK"
fi

# ---- patched source + release build (the launcher's optimized profile)
STAGE=$WORK/stage
rm -rf "$STAGE"; mkdir -p "$STAGE/sunset"
copy "$MOD" "$STAGE/sunset" /.horizon /target
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

# ---- the LGPL corresponding source as a patch series on the pin (local branch; pushing is a manual step), released
# as a tarball of the whole tree and a git bundle of the series; and the third-party licences of that tree
NAME=sunset-$VERSION-$SHORT-$PLATFORM
LIC=$WORK/licences; rm -rf "$LIC"; mkdir -p "$LIC"
{
	BR=sunset/$SHORT
	git -C "$FORK" cat-file -e "$REV^{commit}" 2>/dev/null || git -C "$FORK" fetch -q origin "$REV"
	FW=$WORK/fork-tree; rm -rf "$FW"
	git -C "$FORK" worktree prune
	git -C "$FORK" worktree add -q -f -B "$BR" "$FW" "${HOOKS_HEAD:-$REV}"
	# The tree the shipped Horizon libraries are built from, before Sunset's commits: its whole dependency tree.
	RUSTDIR=$(ls -d "$LDATA/rust/$RUST"-* | head -1)
	say "third-party licences (cargo-about)"
	FLAGS=$WORK/licence-flags.txt "$REPO/client/release/third-party.sh" prebuilt "$FW" "$RUSTDIR" "$LIC/THIRD-PARTY-LICENSES.txt"
	cp "$FW/LICENSES/MIT.txt" "$LIC/HORIZON-MIT.txt"
	export GIT_AUTHOR_DATE="@$(git -C "$REPO" log -1 --format=%ct)" GIT_COMMITTER_DATE="@$(git -C "$REPO" log -1 --format=%ct)"
	gc() { git -C "$FW" add -A sunset && git -C "$FW" commit -q -m "$1" -m "$2"; }
	mkdir -p "$FW/sunset/package" "$FW/sunset/patch"
	copy "$STAGE/sunset" "$FW/sunset/package"
	cp "$REPO/LICENSE" "$FW/sunset/package/LICENSE"
	gc "Sunset $VERSION: the .hmod package" "client/sunset of PointOfPressure/sunset-client $SRC_COMMIT$($DIRTY && echo ' (with local changes)'), under its own licence (sunset/package/LICENSE), not Horizon's."
	copy "$BUILD/patch-workspace" "$FW/sunset/patch" /.horizon-mixins-stage.json /native-post-build.json /target
	gc "Function-replacement crates of $SHORT, as generated" "What horizon-launcher $LAUNCHER_VERSION native-build extracts from this revision's horizon-client, horizon-client-lib, -render and -input for the replaced functions, before any mod is applied. Unmodified Horizon code (LGPL-3.0-or-later)."
	copy "$BUILD/patch-applied" "$FW/sunset/patch" /target
	cp "$BUILD/horizon_patch.rs" "$FW/sunset/patch/horizon_patch.rs"
	gc "Sunset's mixins woven in (horizon-mixins ${MIXINS:0:7})" "The source of $PATCH_LIB as shipped: sunset/package/horizon-mod.toml's mixins and support modules applied by horizon-mixins, and the launcher's entry module (horizon_patch.rs). Rust $RUST. A modified version of Horizon: LGPL-3.0-or-later, the woven-in code included."
	sed -e "s/@VERSION@/$VERSION/g; s/@REV@/$REV/g; s/@TAG@/$TAG/g; s/@LAUNCHER@/$LAUNCHER_VERSION/g; s/@MIXINS@/$MIXINS/g; s/@RUST@/$RUST/g; s/@SRC@/$SRC_COMMIT/g" \
		"$REPO/client/horizon-patches/fork-README.md" | dllnames > "$FW/sunset/README.md"
	gc "Sunset: how this branch is built and licensed" "sunset/README.md."
	FORK_HEAD=$(git -C "$FW" rev-parse HEAD)
	P=$REPO/client/horizon-patches/$SHORT
	rm -rf "$P"; mkdir -p "$P"
	# The weave and the notes are the readable part; the package is client/sunset itself and the extract is generated.
	git -C "$FW" format-patch -q --no-signature -o "$P" "$BR~2..$BR"
	git -C "$FW" log --reverse --format='%H %s' "$REV..$BR" > "$P/series"
	# Sunset's hook patches on the pin (the fork SDK's source), one per feature.
	[ -n "${HOOKS_HEAD:-}" ] && git -C "$FORK" format-patch -q --no-signature -o "$P/hooks" "$REV..$HOOKS_HEAD"
	git -C "$FW" archive --format=tar --prefix="horizon-$SHORT-sunset/" HEAD | zstd -q -19 -T0 -f -o "$OUT/$NAME-horizon-source.tar.zst"
	git -C "$FORK" bundle create -q "$OUT/$NAME-horizon-series.bundle" "$REV..$BR"
	git -C "$FORK" worktree remove --force "$FW"
	say "fork branch $BR = ${FORK_HEAD:0:10} (local, not pushed: push it before publishing); patches in client/horizon-patches/$SHORT"
}

# ---- licences: in the game folder (licences/), in the relink kit and beside the release
sed -e "s/@VERSION@/$VERSION/g; s/@PLATFORM@/$PLATFORM/g; s/@REV@/$REV/g; s/@TAG@/$TAG/g; s/@SRC@/$SRC_COMMIT/g" \
	-e "s/@FORK@/$FORK_HEAD/g; s/@SHORT@/$SHORT/g; s/@NAME@/$NAME/g" "$REPO/client/release/HORIZON-NOTICE.txt" | dllnames > "$LIC/HORIZON-NOTICE.txt"
dllnames < "$REPO/client/release/PREBUILT-TERMS.md" > "$LIC/PREBUILT-TERMS.md"
cp "$REPO/client/release/LICENSE-LGPL-3.0.txt" "$REPO/client/release/LICENSE-GPL-3.0.txt" "$LIC/"
cp "$REPO/LICENSE" "$LIC/SUNSET-LICENSE.txt"
# The launcher embeds its own list: regenerated here so a release never ships a stale one.
cp "$REPO/launcher/THIRD-PARTY-LICENSES.txt" "$WORK/launcher-licences.old"
FLAGS=$WORK/launcher-licence-flags.txt "$REPO/client/release/third-party.sh" launcher
cmp -s "$WORK/launcher-licences.old" "$REPO/launcher/THIRD-PARTY-LICENSES.txt" ||
	say "launcher/THIRD-PARTY-LICENSES.txt changed: commit it and rebuild the launcher"
LICENCE_FILES="LICENSE-LGPL-3.0.txt LICENSE-GPL-3.0.txt HORIZON-MIT.txt PREBUILT-TERMS.md SUNSET-LICENSE.txt $NAME-HORIZON-NOTICE.txt $NAME-THIRD-PARTY-LICENSES.txt sunset-launcher-THIRD-PARTY-LICENSES.txt"
for f in LICENSE-LGPL-3.0.txt LICENSE-GPL-3.0.txt HORIZON-MIT.txt PREBUILT-TERMS.md SUNSET-LICENSE.txt; do cp "$LIC/$f" "$OUT/$f"; done
cp "$LIC/HORIZON-NOTICE.txt" "$OUT/$NAME-HORIZON-NOTICE.txt"
cp "$LIC/THIRD-PARTY-LICENSES.txt" "$OUT/$NAME-THIRD-PARTY-LICENSES.txt"
cp "$REPO/launcher/THIRD-PARTY-LICENSES.txt" "$OUT/sunset-launcher-THIRD-PARTY-LICENSES.txt"

# ---- package: the version folder as the launcher installs it, minus Horizon's developer tools, anything prepared
# from Mojang's files (.local) and Sunset's Rust source. The game checks each exported package at start
# (horizon-mod-package runtime.rs: horizon-mod.toml's id, version and side, and that Cargo.toml and src/ are both
# there), so horizon-native-mods/<id>/ keeps its manifest, Cargo.toml and assets with an empty src/.
PKG=$WORK/pkg/$VDIR
rm -rf "$WORK/pkg"; mkdir -p "$PKG"
copy "$V" "$PKG" /.local /tools '/horizon-native-mods/*/src/*'
[ -z "$(find "$PKG/horizon-native-mods" -name '*.rs')" ] || die "Rust source left in the package"
cp -r "$LIC" "$PKG/licences"
(cd "$PKG" && find . -type f ! -name files.sha256 -printf '%P\n' | LC_ALL=C sort | xargs -d '\n' sha256sum | sums_fix > files.sha256)
UNPACKED=$(du -sb "$PKG" | cut -f1)
tar -C "$WORK/pkg" --owner=0 --group=0 --numeric-owner --mtime=@"$(git -C "$REPO" log -1 --format=%ct)" --sort=name \
	-cf - "$VDIR" | zstd -q -19 -T0 -f -o "$OUT/$NAME.tar.zst"

# ---- plugin kit (see the header): the launcher downloads it only when plugins are turned on
MTIME=@"$(git -C "$REPO" log -1 --format=%ct)"
tar -C "$STAGE" --owner=0 --group=0 --numeric-owner --mtime="$MTIME" --sort=name --exclude=sunset/.horizon --exclude=sunset/target \
	-cf - sunset | zstd -q -19 -T0 -f -o "$OUT/$NAME-source.tar.zst"
SDKNAME=
if [ -n "${HOOKS:-}" ]; then
	SDKNAME=sunset-sdk-$SHORT-${HOOKS_HEAD:0:10}-$PLATFORM
	tar -C "$(dirname "$SDK")" --owner=0 --group=0 --numeric-owner --mtime="$MTIME" --sort=name \
		--transform "s,^$(basename "$SDK"),fork-sdk," -cf - "$(basename "$SDK")" | zstd -q -19 -T0 -f -o "$OUT/$SDKNAME.tar.zst"
fi

# ---- relink kit: the objects of Sunset's two libraries and their link inputs (see horizon-patches/README.md)
if [ "${RELINK:-1}" != 0 ]; then
	KIT=$WORK/relink/$NAME-relink; rm -rf "$WORK/relink"; mkdir -p "$KIT"
	# Every mod library too: the patched rlibs depend on the mods they call ($CORE_LIB), found beside them; on
	# Windows with their import libraries (lib*.dll.a).
	cp "$BUILD"/*.rlib "$BUILD"/*.externs "$BUILD"/*.identity.json "$BUILD"/*$DLL "$BUILD"/horizon_patch.rs "$KIT/"
	[ "$PLATFORM" != windows-x86_64 ] || cp "$BUILD"/lib*.dll.a "$KIT/"
	# The launcher writes Windows paths with backslashes: one spelling (C:/...) before the folders become $SDK and $KIT.
	[ "$PLATFORM" != windows-x86_64 ] || sed -i 's|\\|/|g' "$KIT"/*.externs
	sed -i "s|$(native_path "$LDATA/native/prebuilt-package")|\$SDK|g; s|$(native_path "$BUILD")|\$KIT|g" "$KIT"/*.externs
	! grep -q "$(native_path "$WORK")" "$KIT"/*.externs || die "relink kit: build paths left in $KIT's .externs"
	# The patch library's own --extern lines, as horizon-launcher $LAUNCHER_VERSION passes them (native_mods/prebuilt.rs
	# wrapper_externs: the SDK's shared library of each crate, else its one rmeta; then the patched rlibs).
	SDKP=$LDATA/native/prebuilt-package
	for c in horizon_client horizon_mod_host horizon_mod_package mod_api mod_points mod_vocab; do
		if [ -f "$SDKP/deps/$DLL_PREFIX$c$DLL" ]; then f=$DLL_PREFIX$c$DLL; else f=$(cd "$SDKP/deps" && echo lib$c-*.rmeta); fi
		[ -f "$SDKP/deps/$f" ] || die "relink kit: no single library for $c in $SDKP/deps ($f)"
		printf -- '--extern\n%s=$SDK/deps/%s\n' "$c" "$f"
	done > "$KIT/horizon_patch.link"
	for r in "$BUILD"/libhorizon_patched*.rlib; do n=$(basename "$r" .rlib); printf -- '--extern\n%s=$KIT/%s.rlib\n' "${n#lib}" "$n"; done >> "$KIT/horizon_patch.link"
	echo "$RUST" > "$KIT/RUST"; echo "$PLATFORM" > "$KIT/PLATFORM"
	cp "$REPO/client/release/relink/relink.sh" "$KIT/"
	sed -e "s/@VERSION@/$VERSION/g; s/@SHORT@/$SHORT/g; s/@PLATFORM@/$PLATFORM/g; s/@RUST@/$RUST/g" "$REPO/client/release/relink/README.md" | dllnames > "$KIT/README.md"
	cp "$LIC/HORIZON-NOTICE.txt" "$LIC/PREBUILT-TERMS.md" "$LIC/LICENSE-LGPL-3.0.txt" "$LIC/LICENSE-GPL-3.0.txt" "$KIT/"
	tar -C "$WORK/relink" --owner=0 --group=0 --numeric-owner --sort=name -cf - "$NAME-relink" | zstd -q -19 -T0 -f -o "$OUT/$NAME-relink.tar.zst"
fi

# ---- manifest + signatures
# The Sunset launcher downloads the same stock CLI from upstream and checks it against this signed hash.
case $PLATFORM in windows-x86_64) CLI_FILE=horizon-launcher-windows-x64.zip ;; *) CLI_FILE=horizon-launcher-$PLATFORM.tar.gz ;; esac
CLI_URL=https://github.com/horizon-rust/horizon/releases/download/launcher@$LAUNCHER_VERSION/$CLI_FILE
CLI_SHA=$(curl -fsSL "https://github.com/horizon-rust/horizon/releases/download/launcher@$LAUNCHER_VERSION/SHA256SUMS" | awk -v f="$CLI_FILE" '$2 == f || $2 == "*" f {print $1}')
[ ${#CLI_SHA} = 64 ] || die "no SHA-256 for $CLI_FILE in launcher@$LAUNCHER_VERSION's SHA256SUMS"
# The launcher shows this version's client/CHANGELOG.md section on Overview (empty when it has none).
CHANGELOG_JSON=$(python3 "$REPO/client/scripts/changelog.py" "$VERSION" "$REPO/client/CHANGELOG.md")
[ "$CHANGELOG_JSON" != "[]" ] || say "client/CHANGELOG.md has no $VERSION section: the launcher will show no changelog"
python3 - "$OUT" "$NAME" "$VDIR" "$UNPACKED" "$CHANGELOG_JSON" "$SDKNAME" <<EOF
import hashlib, json, os, sys, datetime
out, name, vdir, unpacked = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
changelog = json.loads(sys.argv[5])
sdkname = sys.argv[6]
def art(f, kind):
    p = os.path.join(out, f)
    h = hashlib.sha256(open(p, "rb").read()).hexdigest()
    return {"file": f, "kind": kind, "size": os.path.getsize(p), "sha256": h, "signature": f + ".minisig"}
arts = [art(name + ".tar.zst", "game")]
arts[0]["unpacked_size"] = unpacked
if os.path.exists(os.path.join(out, name + "-relink.tar.zst")):
    arts.append(art(name + "-relink.tar.zst", "relink"))
arts.append(art(name + "-source.tar.zst", "source"))
if sdkname:
    arts.append(art(sdkname + ".tar.zst", "sdk"))
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
if changelog:
    m["changelog"] = changelog
with open(os.path.join(out, "manifest.json"), "w", newline="\n") as f:
    json.dump(m, f, indent=2)
    f.write("\n")
EOF
KIT_FILES="$NAME-source.tar.zst${SDKNAME:+ $SDKNAME.tar.zst} $NAME-horizon-source.tar.zst $NAME-horizon-series.bundle $LICENCE_FILES"
(cd "$OUT" && sums manifest.json "$NAME".tar.zst $(ls "$NAME"-relink.tar.zst 2>/dev/null) $KIT_FILES > SHA256SUMS)
if [ "${SIGN:-1}" != 0 ]; then
	for f in "$NAME.tar.zst" $(cd "$OUT" && ls "$NAME"-relink.tar.zst 2>/dev/null) $KIT_FILES manifest.json; do
		minisign -S -s "$KEY" -m "$OUT/$f" -t "sunset $VERSION $SHORT $PLATFORM $f" >/dev/null </dev/null
	done
	minisign -V -q -p "$REPO/launcher/sunset.pub" -m "$OUT/manifest.json" || die "signature check failed"
fi
say "done: $OUT ($(du -h "$OUT/$NAME.tar.zst" | cut -f1) package, $(( UNPACKED / 1048576 )) MiB unpacked)"
