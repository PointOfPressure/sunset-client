#!/bin/sh
# Installs Solstice into Horizon, builds it into the game and starts it. One command:
#
#   curl -fsSL https://github.com/PointOfPressure/solstice-client/releases/latest/download/install.sh | sh
#
# Options (after `sh -s --` when piping): --no-build (only put the file in place), --no-play (build, don't start),
# and, from a downloaded copy: sh install.sh [solstice.hnmod] [game folder]. SOLSTICE_DRY=1 shows what it would run.
set -e

build=1
play=1
file=""
home=""
for arg in "$@"; do
    case "$arg" in
        --no-build) build=0; play=0 ;;
        --no-play) play=0 ;;
        --build) build=1 ;;
        "") ;;
        *) if [ -z "$file" ] && [ -f "$arg" ]; then file=$arg; else home=$arg; fi ;;
    esac
done

say() { printf '%s\n' "$*"; }

# Where Horizon keeps the game's files: HORIZON_HOME, else its folder in the platform's data directory.
case "$(uname -s)" in
    Darwin) data="${HORIZON_HOME:-$HOME/Library/Application Support/horizon}" ;;
    *) data="${HORIZON_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/horizon}" ;;
esac
if [ -n "$home" ]; then
    homes=$home
else
    # The default game folder, and the Native mods instance once a build made it.
    homes=$data
    [ -d "$data/instances/native-mods" ] && homes="$homes
$data/instances/native-mods"
fi

# The launcher's program: on the PATH, in its usual places, or where its menu entry points.
find_launcher() {
    command -v horizon-launcher 2>/dev/null && return
    for exe in /Applications/Horizon*.app/Contents/MacOS/horizon-launcher "$HOME"/Applications/Horizon*.app/Contents/MacOS/horizon-launcher; do
        [ -x "$exe" ] && echo "$exe" && return
    done
    for dir in "$HOME/.local/bin" "$HOME/bin" /usr/local/bin /opt/horizon-launcher; do
        [ -x "$dir/horizon-launcher" ] && echo "$dir/horizon-launcher" && return
    done
    entry="${XDG_DATA_HOME:-$HOME/.local/share}/applications/horizon-launcher.desktop"
    if [ -f "$entry" ]; then
        exe=$(sed -n 's/^Exec=//p' "$entry" | head -1)
        case "$exe" in
            \"*) exe=${exe#\"}; exe=${exe%%\"*} ;;
            *) exe=${exe%% *} ;;
        esac
        [ -x "$exe" ] && echo "$exe" && return
    fi
    for exe in "$HOME"/Downloads/horizon-launcher*/horizon-launcher "$HOME"/Applications/horizon-launcher* "$HOME"/horizon-launcher*/horizon-launcher; do
        [ -x "$exe" ] && echo "$exe" && return
    done
    return 1
}

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
say "Solstice: getting the mod..."
if [ -n "$file" ]; then
    cp "$file" "$tmp"
else
    curl -fsSL -o "$tmp" https://github.com/PointOfPressure/solstice-client/releases/latest/download/solstice.hnmod
fi

echo "$homes" | while IFS= read -r h; do
    [ -n "$h" ] || continue
    mkdir -p "$h/horizon-native-mods"
    cp "$tmp" "$h/horizon-native-mods/solstice.hnmod"
    # An older name of the same mod.
    rm -f "$h/horizon-native-mods/meteor.hnmod" "$h/horizon-native-mods/delta.hnmod"
    list="$h/horizon-mods.txt"
    touch "$list"
    grep -v -e '^mod\.solstice:' -e '^mod\.meteor:' -e '^mod\.delta:' "$list" > "$list.tmp" || true
    echo 'mod.solstice: true' >> "$list.tmp"
    mv "$list.tmp" "$list"
done
say "Solstice: installed."

# Native mods are compiled into the game together, so they all have to be made for the same Horizon version.
others=""
echo "$homes" | {
    while IFS= read -r h; do
        [ -d "$h/horizon-native-mods" ] || continue
        for f in "$h"/horizon-native-mods/*.hnmod; do
            [ -f "$f" ] || continue
            n=$(basename "$f" .hnmod)
            [ "$n" = solstice ] || echo "$n"
        done
    done
} | sort -u > "$tmp.others"
if [ -s "$tmp.others" ]; then
    say ""
    say "You also have other native mods: $(tr '\n' ' ' < "$tmp.others")"
    say "They are built into the game together with Solstice, so they must be made for the same Horizon version (Solstice 0.8.0 is made for Nightly ba6344a)."
    say "If the build says 'made for different Horizon revisions', turn the others off in the launcher's Mods page, or delete their .hnmod from horizon-native-mods, and run this again."
    say ""
fi
rm -f "$tmp.others"

if [ "$build" = 0 ]; then
    say "Next: in the Horizon launcher, Settings > Game > Native mods (experimental) on, then Build. Or run this again without --no-build."
    exit 0
fi

launcher=$(find_launcher) || {
    say ""
    say "I couldn't find the Horizon launcher. Install it from https://playhorizon.dev, open it once, then run this command again."
    exit 1
}

# Linux needs the sound and device headers to compile the game. Install them if they're missing.
if [ "$(uname -s)" = Linux ] && command -v pkg-config >/dev/null 2>&1 && ! pkg-config --exists alsa libudev 2>/dev/null; then
    say "Solstice: the build needs two system libraries (sound and device headers). Installing them (it may ask for your password)..."
    if [ -n "$SOLSTICE_DRY" ]; then
        say "would install the libraries"
    elif command -v apt-get >/dev/null 2>&1; then
        sudo apt-get install -y libasound2-dev libudev-dev pkg-config
    elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y alsa-lib-devel systemd-devel pkgconf-pkg-config
    elif command -v pacman >/dev/null 2>&1; then
        sudo pacman -S --needed --noconfirm alsa-lib systemd pkgconf
    elif command -v zypper >/dev/null 2>&1; then
        sudo zypper install -y alsa-devel systemd-devel pkg-config
    else
        say "Install your system's ALSA and libudev development packages, then run this again."
        exit 1
    fi
fi

say "Solstice: building it into the game (10 to 30 minutes the first time, a few after that)..."
if [ -n "$SOLSTICE_DRY" ]; then
    say "would run: $launcher native-build --trust-native-code"
    [ "$play" = 1 ] && say "would run: $launcher play --instance native-mods"
    exit 0
fi
if ! "$launcher" native-build --trust-native-code; then
    say ""
    say "The build failed. If it said the mods were 'made for different Horizon revisions', you have another native mod built for a different Horizon version: turn it off in the launcher's Mods page (or delete its .hnmod from horizon-native-mods) and run this again."
    exit 1
fi
say "Solstice: done."
if [ "$play" = 1 ]; then
    say "Starting Horizon (the \"Native mods\" instance). In the game, press the \` key."
    nohup "$launcher" play --instance native-mods >/dev/null 2>&1 &
else
    say "Play the \"Native mods\" instance in the Horizon launcher. In the game, press the \` key."
fi
