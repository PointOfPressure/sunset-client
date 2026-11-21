#!/bin/bash
# fork-sdk.sh: the mod SDK of Sunset's Horizon fork (PointOfPressure/horizon sunset-hooks/<pin>: upstream <pin> plus
# the hook patches in client/horizon-patches/<pin>/hooks/), built as build id <pin>, and a launcher home seeded with it.
#
#   fork-sdk.sh build             FORK=<worktree at the hooks branch>: SDK assets into $SDK/sdk-<pin>/ (this OS's client
#                                 leg: Linux, or Windows in Git Bash)
#   fork-sdk.sh seed <home>       the launcher home <home> (its .local/share/horizon-launcher) set to use $SDK's SDK
#                                 and the fork's source for <pin> instead of the published ones
#
# Why it works: the launcher keeps a verified sdk.json under native/sdk/<build id>/ and trusts it again without the
# signature, fetches the slices from HORIZON_SDK_BASE (a folder) against that index, and reuses native/source/<rev>/
# once it holds .horizon-revision; sdk/versions/<Nightly> keeps the build id the Nightly's release names. The build id stays the pin (tools/prebuilt_game.py HORIZON_SDK_BUILD_ID, fork
# commit "tools: build the SDK of a patched branch as the build it patches"), so the instance's Nightly, the
# package's [native] pin and the game's own build id all agree. Rust: the launcher's own (rust/<version>), which the
# SDK's rlibs must match. Env: SDK (default ~/sunset-ops/tmp/fork-sdk), CARGO_TARGET_DIR (default
# ~/sunset-ops/tmp/target-sdk), RUST_BIN (the launcher's toolchain bin folder).
set -euo pipefail
REPO=$(cd "$(dirname "$0")/../.." && pwd)
# Windows' python3 is often the Microsoft Store's stub.
PY=python3; $PY -c '' 2>/dev/null || PY=python
REV=$($PY -c 'import sys,tomllib;print(tomllib.load(open(sys.argv[1],"rb"))["native"]["horizon"])' "$REPO/client/sunset/horizon-mod.toml")
SDK=${SDK:-$HOME/sunset-ops/tmp/fork-sdk}
say() { echo "fork-sdk: $*" >&2; }
die() { say "$*"; exit 1; }

case "${1:-}" in
build)
	F=${FORK:?FORK=<horizon worktree at the hooks branch>}
	[ -n "${RUST_BIN:-}" ] && export PATH=$RUST_BIN:$PATH
	export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HOME/sunset-ops/tmp/target-sdk} HORIZON_SDK_BUILD_ID=$REV
	git -C "$F" merge-base --is-ancestor "$REV" HEAD || die "$F is not based on $REV"
	say "building the SDK of $(git -C "$F" rev-parse --short HEAD) as $REV ($(rustc -V))"
	W=$CARGO_TARGET_DIR/fork-sdk; rm -rf "$W"; mkdir -p "$W"
	case $(uname -s) in MINGW*|MSYS*|CYGWIN*) LEG=windows ;; *) LEG=linux ;; esac
	(cd "$F" && $PY tools/prebuilt_game.py build "$W/out" && $PY tools/prebuilt_game.py pack "$W/out" "$W/legs/$LEG" \
		&& $PY tools/prebuilt_game.py gather "$W/legs" "$W/dist" && $PY tools/prebuilt_game.py index "$W/dist")
	rm -rf "$SDK"; mkdir -p "$SDK"; mv "$W/dist" "$SDK/sdk-$REV"
	git -C "$F" -c core.autocrlf=false archive --format=tar --prefix=source/ HEAD | tar -x -C "$SDK"
	find "$SDK/source" -type l -delete  # as the launcher's own download leaves links out
	echo "$REV" > "$SDK/source/.horizon-revision"; git -C "$F" rev-parse HEAD > "$SDK/fork-commit"
	say "SDK in $SDK/sdk-$REV, source of $(cat "$SDK/fork-commit" | cut -c1-10) in $SDK/source"
	;;
seed)
	H=${2:?seed <launcher home>}; N=$H/.local/share/horizon-launcher/native
	[ -f "$SDK/sdk-$REV/sdk.json" ] || die "no SDK in $SDK (fork-sdk.sh build first)"
	mkdir -p "$N/sdk" "$N/source"
	rm -rf "$N/sdk/$REV" "$N/prebuilt/$REV"-* "$N/prebuilt-package" "$N/source/$REV"
	mkdir -p "$N/sdk/$REV" "$N/sdk/versions"; cp "$SDK/sdk-$REV/sdk.json" "$N/sdk/$REV/sdk.json"
	# The Nightly's own build id, as its signed release would name it (read from HORIZON_SDK_BASE otherwise).
	for v in "$H"/.local/share/horizon-launcher/versions/nightly-*-"${REV:0:7}"; do
		[ -d "$v" ] && echo "$REV" > "$N/sdk/versions/$(basename "$v")"
	done
	cp -a "$SDK/source" "$N/source/$REV"; cp "$SDK/fork-commit" "$N/.fork-seeded"
	say "seeded $N with the fork SDK ($(cut -c1-10 "$SDK/fork-commit")); run the launcher with HORIZON_SDK_BASE=$SDK"
	;;
*) sed -n 2,8p "$0"; exit 2 ;;
esac
