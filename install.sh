#!/bin/sh
# Installs Solstice into Horizon's default instance and turns it on,
# so the launcher's Build button (Settings > Game > Native mods) builds it in.
#   curl -fsSL https://github.com/PointOfPressure/solstice-client/releases/latest/download/install.sh | sh
# or, with a downloaded file: ./install.sh solstice-<version>.hnmod [instance folder]
set -e
home=${2:-"${XDG_DATA_HOME:-$HOME/.local/share}/horizon"}
mkdir -p "$home/horizon-native-mods"
dest="$home/horizon-native-mods/solstice.hnmod"
if [ -n "$1" ]; then
    cp "$1" "$dest"
else
    curl -fsSL -o "$dest" https://github.com/PointOfPressure/solstice-client/releases/latest/download/solstice.hnmod
fi
rm -f "$home/horizon-native-mods/meteor.hnmod" "$home/horizon-native-mods/delta.hnmod"
list="$home/horizon-mods.txt"
touch "$list"
grep -v -e '^mod\.solstice:' -e '^mod\.meteor:' -e '^mod\.delta:' "$list" > "$list.tmp" || true
echo 'mod.solstice: true' >> "$list.tmp"
mv "$list.tmp" "$list"
echo "Solstice installed. In the Horizon launcher: Settings > Game > Native mods (experimental) on, then Build."
