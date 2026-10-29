#!/bin/sh
# Installs Delta Client into Horizon's default instance and turns it on,
# so the launcher's Build button (Settings > Game > Native mods) builds it in.
#   curl -fsSL https://github.com/PointOfPressure/delta-client/releases/latest/download/install.sh | sh
# or, with a downloaded file: ./install.sh delta-<version>.hnmod [instance folder]
set -e
home=${2:-"${XDG_DATA_HOME:-$HOME/.local/share}/horizon"}
mkdir -p "$home/horizon-native-mods"
dest="$home/horizon-native-mods/delta.hnmod"
if [ -n "$1" ]; then
    cp "$1" "$dest"
else
    curl -fsSL -o "$dest" https://github.com/PointOfPressure/delta-client/releases/latest/download/delta.hnmod
fi
rm -f "$home/horizon-native-mods/meteor.hnmod"
list="$home/horizon-mods.txt"
touch "$list"
grep -v -e '^mod\.delta:' -e '^mod\.meteor:' "$list" > "$list.tmp" || true
echo 'mod.delta: true' >> "$list.tmp"
mv "$list.tmp" "$list"
echo "Delta Client installed. In the Horizon launcher: Settings > Game > Native mods (experimental) on, then Build."
