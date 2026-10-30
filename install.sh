#!/bin/sh
# Installs Solstice into Horizon, and with --build compiles it into the game.
#   curl -fsSL https://github.com/PointOfPressure/solstice-client/releases/latest/download/install.sh | sh -s -- --build
# Or with a downloaded file: sh install.sh [--build] [solstice.hnmod] [instance folder]
# SOLSTICE_DRY=1 shows the build command instead of running it.
set -e

build=0
file=""
home=""
for arg in "$@"; do
    case "$arg" in
        --build) build=1 ;;
        "") ;;
        *) if [ -z "$file" ] && [ -f "$arg" ]; then file=$arg; else home=$arg; fi ;;
    esac
done

data="${XDG_DATA_HOME:-$HOME/.local/share}/horizon"
if [ -n "$home" ]; then
    homes=$home
else
    # The default game folder, and the Native mods instance once a build made it.
    homes=$data
    [ -d "$data/instances/native-mods" ] && homes="$homes $data/instances/native-mods"
fi

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
if [ -n "$file" ]; then
    cp "$file" "$tmp"
else
    curl -fsSL -o "$tmp" https://github.com/PointOfPressure/solstice-client/releases/latest/download/solstice.hnmod
fi

for h in $homes; do
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
echo "Solstice installed."

# The launcher's program: on the PATH, in its usual places, or where its menu entry points.
find_launcher() {
    command -v horizon-launcher 2>/dev/null && return
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
    for exe in "$HOME"/Downloads/horizon-launcher*/horizon-launcher "$HOME"/Applications/horizon-launcher*; do
        [ -x "$exe" ] && echo "$exe" && return
    done
    return 1
}

if [ "$build" = 0 ]; then
    echo "Next: in the Horizon launcher, Settings > Game > Native mods (experimental) on, then Build."
    echo "Or run this again with --build to do that from here."
    exit 0
fi

launcher=$(find_launcher) || {
    echo "Could not find the Horizon launcher. In it: Settings > Game > Native mods (experimental) on, then Build."
    exit 1
}
if [ "$(uname -s)" = Linux ] && command -v pkg-config >/dev/null 2>&1; then
    if ! pkg-config --exists alsa libudev 2>/dev/null; then
        echo "The build needs the sound and device headers once:"
        echo "  sudo apt install libasound2-dev libudev-dev pkg-config"
        exit 1
    fi
fi
echo "Compiling Solstice into the game with $launcher."
echo "The first build takes 10 to 30 minutes, later ones a few."
if [ -n "$SOLSTICE_DRY" ]; then
    echo "would run: $launcher native-build --trust-native-code"
    exit 0
fi
"$launcher" native-build --trust-native-code
echo "Done. Play the \"Native mods\" instance in the launcher."
