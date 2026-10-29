#!/bin/sh
# Puts a Meteor .hnmod into Horizon's default instance and turns it on, so
# the launcher's Build button (Settings > Game > Native mods) builds it in.
# Usage: ./install.sh meteor-0.1.0.hnmod [instance folder]
set -e
file=${1:?usage: ./install.sh meteor-<version>.hnmod [instance folder]}
home=${2:-"${XDG_DATA_HOME:-$HOME/.local/share}/horizon"}
mkdir -p "$home/horizon-native-mods"
cp "$file" "$home/horizon-native-mods/meteor.hnmod"
list="$home/horizon-mods.txt"
touch "$list"
grep -v '^mod\.meteor:' "$list" > "$list.tmp" || true
echo 'mod.meteor: true' >> "$list.tmp"
mv "$list.tmp" "$list"
echo "Installed into $home. Now in the launcher: Settings > Game > Native mods (experimental) > Build."
