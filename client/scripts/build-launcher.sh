#!/bin/bash
# build-launcher.sh: the Sunset launcher's release files: sunset-launcher-linux-x86_64.tar.gz (one sunset-launcher/
# folder: the program, LICENSE, THIRD-PARTY-LICENSES.txt) and sunset-launcher-windows-x86_64.exe, cross-built from
# Linux (README.md, "Sunset launcher"). build-prebuilt.sh takes either as SUNSET_LAUNCHER: the release's manifest then
# names it, and installed launchers from 2.1.0 on update themselves to it (launcher/src/selfupdate.rs).
#
#   PLATFORMS  which to build (default: "linux-x86_64 windows-x86_64")
#   OUT        where they go (default: <repo>/dist)
#   CARGO_TARGET_DIR, RUSTUP_TOOLCHAIN  as for cargo
set -euo pipefail
REPO=$(cd "$(dirname "$0")/../.." && pwd)
say() { echo "build-launcher: $*" >&2; }
die() { say "$*"; exit 1; }
OUT=${OUT:-$REPO/dist}
T=${CARGO_TARGET_DIR:-$REPO/target}
VERSION=$(python3 -c 'import sys,tomllib;print(tomllib.load(open(sys.argv[1],"rb"))["package"]["version"])' "$REPO/launcher/Cargo.toml")
MTIME=@$(git -C "$REPO" log -1 --format=%ct)
mkdir -p "$OUT"
cd "$REPO"
for p in ${PLATFORMS:-linux-x86_64 windows-x86_64}; do
	case $p in
	linux-x86_64)
		cargo build --locked --release -p sunset-launcher
		s=$(mktemp -d); mkdir "$s/sunset-launcher"
		cp "$T/release/sunset-launcher" "$REPO/LICENSE" "$REPO/launcher/THIRD-PARTY-LICENSES.txt" "$s/sunset-launcher/"
		said=$(SUNSET_NO_SELF_UPDATE=1 "$s/sunset-launcher/sunset-launcher" --version)
		[ "$said" = "sunset-launcher $VERSION" ] || die "the build says $said, launcher/Cargo.toml $VERSION"
		f=$OUT/sunset-launcher-$p.tar.gz
		tar -C "$s" --owner=0 --group=0 --numeric-owner --mtime="$MTIME" --sort=name -cf - sunset-launcher | gzip -9 -n > "$f"
		rm -rf "$s"
		;;
	windows-x86_64)
		cargo build --locked --profile release-cross --target x86_64-pc-windows-gnu -p sunset-launcher
		f=$OUT/sunset-launcher-$p.exe
		cp "$T/x86_64-pc-windows-gnu/release-cross/sunset-launcher.exe" "$f"
		;;
	*) die "PLATFORMS are linux-x86_64 and windows-x86_64, not $p" ;;
	esac
	say "Sunset launcher $VERSION: $f ($(du -h "$f" | cut -f1), sha256 $(sha256sum "$f" | cut -c1-16)...)"
done
