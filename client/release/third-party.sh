#!/bin/bash
# third-party.sh: the THIRD-PARTY-LICENSES file of each binary Sunset ships, generated from the lockfiles with
# cargo-about $ABOUT_VERSION (pinned; about.toml) and rendered by third_party.py, which also prints what needs a look
# (non-permissive or missing licences, MPL-2.0 file-level terms, NOTICE files carried).
#
#   third-party.sh launcher    -> launcher/THIRD-PARTY-LICENSES.txt: the launcher's crates (Linux and Windows builds),
#                                 its Inter font and the Rust standard library of the toolchain that builds it
#   third-party.sh baritone    -> client/baritone-engine/THIRD-PARTY-LICENSES.txt
#   third-party.sh prebuilt TREE RUST OUT
#                              -> OUT for the prebuilt client: Horizon at TREE (a checkout of the commit its libraries
#                                 are built from) with its whole dependency tree and its own notices, Sunset's fonts,
#                                 and the Rust standard library of toolchain folder RUST (shipped as libstd-*.so).
#                                 libsunset_core.so depends only on Horizon's crates, so it is covered too.
# FLAGS=<file> also writes the flags there. build-prebuilt.sh runs the prebuilt and launcher ones.
set -euo pipefail
ABOUT_VERSION=0.9.3
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
die() { echo "third-party.sh: $*" >&2; exit 1; }
[ "$(cargo about --version 2>/dev/null)" = "cargo-about $ABOUT_VERSION" ] ||
	die "needs cargo-about $ABOUT_VERSION: cargo install cargo-about --version $ABOUT_VERSION --locked --features cli"
[ "${1:-}" = check ] && exit 0
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
about() { # manifest json [cargo-about args]
	local m=$1 j=$2; shift 2
	cargo about generate --format json --locked -c "$HERE/about.toml" -m "$m" -o "$j" "$@" 2>"$TMP/about.log" ||
		{ tail -20 "$TMP/about.log" >&2; die "cargo-about failed on $m"; }
}
std_notices() { # toolchain folder
	local f=$1/share/doc/rust/COPYRIGHT-library.html
	[ -f "$f" ] || die "no $f (the Rust standard library's notices; rustup component rust-docs)"
	echo "$f"
}
render() { python3 -I "$HERE/third_party.py" "$@" ${FLAGS:+--flags "$FLAGS"}; }

case ${1:-} in
check) ;;
launcher)
	about "$REPO/launcher/Cargo.toml" "$TMP/l.json"
	render "$REPO/launcher/THIRD-PARTY-LICENSES.txt" "Sunset launcher" "$TMP/l.json" --own "$REPO/launcher" \
		--text "Font: Inter (SIL Open Font License 1.1), embedded in the launcher" "$REPO/assets/licenses/OFL-Inter.txt" \
		--html "Rust standard library $(rustc --version | cut -d' ' -f2) (statically linked)" "$(std_notices "$(rustc --print sysroot)")"
	;;
baritone)
	about "$REPO/client/baritone-engine/Cargo.toml" "$TMP/b.json"
	render "$REPO/client/baritone-engine/THIRD-PARTY-LICENSES.txt" "baritone-engine" "$TMP/b.json" --own "$REPO/client/baritone-engine"
	;;
prebuilt)
	[ $# = 4 ] || die "usage: third-party.sh prebuilt TREE RUST OUT"
	tree=$2 rust=$3 out=$4
	[ -f "$tree/THIRD_PARTY_NOTICES.md" ] || die "$tree is not a Horizon checkout"
	about "$tree/Cargo.toml" "$TMP/h.json" --workspace
	fonts=(); for f in "$REPO"/client/sunset/assets/sunset/licenses/*.txt; do
		fonts+=(--text "Font: $(basename "$f" .txt | sed 's/^ofl-//') (SIL Open Font License 1.1), in ${DLL_PREFIX-lib}sunset_core${DLL:-.so} (subset)" "$f"); done
	render "$out" "Sunset prebuilt client (Horizon $(git -C "$tree" rev-parse --short=10 HEAD) and Sunset)" "$TMP/h.json" --own "$tree" \
		--text "Horizon's own third-party notices (THIRD_PARTY_NOTICES.md at $(git -C "$tree" rev-parse HEAD))" "$tree/THIRD_PARTY_NOTICES.md" \
		"${fonts[@]}" \
		--html "Rust standard library $(basename "$rust" | cut -d- -f1) (${DLL_PREFIX-lib}std-*${DLL:-.so})" "$(std_notices "$rust")"
	;;
*) die "usage: third-party.sh launcher | baritone | prebuilt TREE RUST OUT | check" ;;
esac
