# Solstice

Horizon's first cheat client: a utility client for
[Horizon](https://github.com/horizon-rust/horizon), the Rust rewrite of Minecraft: Java Edition 26.3.
Vibecoded with Claude Code.

It's a Horizon *native mod* (a mixin bundle the launcher compiles into the game) in the style of
[Meteor Client](https://github.com/MeteorDevelopment/meteor-client): modules in categories,
settings, key binds, a menu and `.` chat commands. Horizon can't run Java mods, so it's new
Rust code, with Meteor as the reference for what the modules do. Not made by, or affiliated
with, MeteorDevelopment or the Horizon project.

> Using these on servers that don't allow them will get you banned. Play where it's allowed.

## Install

1. Open a terminal and run:
   ```sh
   curl -fsSL https://github.com/PointOfPressure/solstice-client/releases/latest/download/install.sh | sh
   ```
2. In the Horizon launcher: **Settings > Game > Native mods (experimental)**, switch it on,
   press **Build**, then **I trust these mods: build**. The first build takes 10 to 30 minutes
   (it downloads Rust and compiles the game); later ones take a few minutes.
   On Linux the build needs the sound and device headers once:
   `sudo apt install libasound2-dev libudev-dev pkg-config` (Debian, Ubuntu, Zorin, Mint).
3. Press **Play**, then **Right Shift** in game opens the menu. `.help` in chat lists the commands.

Why step 1 is a command: Horizon's launcher has no button for adding a native mod from a file
(only its own registry installs them). The script puts `solstice.hnmod` in
`~/.local/share/horizon/horizon-native-mods/` and turns it on in `horizon-mods.txt`. With a
downloaded file instead: `sh install.sh solstice.hnmod [instance folder]`.

To update, run the same command and press Build again. To remove it, turn it off
in Options > Mods (it does nothing from then on), or delete the file and pick the plain
Nightly again in the launcher.

Each version is pinned to one Horizon Nightly (0.7.0: `ba6344a`) (`horizon` in `horizon-native.json`) because it
hooks the game's own functions. The launcher builds it on that commit, so it keeps working
after Horizon updates, but plays that Nightly's game until the mod moves to a newer one.

## Use

- **Right Shift** opens the click GUI (rebind it under Options > Mods > Solstice): every
  category as a panel. Left click turns a module on or off, right click opens its settings
  (sliders for numbers, right click a slider to reset it), the Bind row takes the next key.
  Drag the headers to move panels, right click a header to fold it, scroll a long panel with the wheel. Esc closes it. The game
  keeps running while it's open.
- The same settings are also pages under Options > Mods > Solstice (`.pages`).
- Just type to search the modules while the GUI is open. Esc clears the search, then closes the GUI.
- **Key binds** toggle a module while you play.
- **Chat commands** start with `.` and never reach the server (`..text` sends `.text`):

| Command | Does |
| --- | --- |
| `.help` | list the commands |
| `.menu` | open the menu |
| `.t <module>` | toggle a module (`.toggle`) |
| `.modules` | list every module, `*` on |
| `.settings <module>` | list a module's settings |
| `.set <module> <setting> <value>` | change one (`.set killaura range 4`); the start of a setting's name is enough (`.set aimassist cone 60`); a value can be several words (`.set spam messages hello | hi`); lists like Xray's blocks are set this way (`.set xray blocks diamond_ore,ancient_debris`) |
| `.bind <module> <key\|none>` | bind a key (`r`, `rshift`, `f6`...) |
| `.binds` | list key binds |
| `.reset <module>` | back to defaults |
| `.friend add\|remove\|list [name]` | Kill Aura and Trigger Bot skip friends |
| `.waypoint add\|remove\|list [name]` | save places; shown on screen by the Waypoints module (`.wp`) |
| `.vclip <blocks>`, `.hclip <blocks>` | move up or down, or forward |

Settings and binds are saved in `horizon-mods/.horizon-mod-data/solstice/config`.

## Modules

| Category | Modules |
| --- | --- |
| Combat | Kill Aura, Trigger Bot, Criticals, Aim Assist, Auto Totem, Auto Weapon, Auto Log, Auto Armor, Offhand, Auto EXP, Bow Spam, Bow Aimbot, Surround, Self Trap, Auto Trap, Self Web, Auto Web, Hole Filler, Burrow |
| Movement | Sprint, Sneak, Auto Walk, Flight, Speed, No Fall, No Slow, Velocity, Spider, Step, Air Jump, Elytra Boost, Anti Void, Safe Walk, Fast Climb, High Jump, Long Jump, Parkour, Blink, Scaffold, Reverse Step, Auto Jump, Chest Swap, Anchor |
| Player | Fast Place, Fast Break, Fast Use, Auto Respawn, Anti Hunger, Anti AFK, Auto Clicker, Auto Eat, Auto Gap, Auto Fish, Auto Replenish, Auto Tool, Middle Click Extra, Reach, Air Place, Notifier, Spam |
| Render | HUD, Fullbright, ESP, Tracers, Nametags, Storage ESP, Zoom, Breadcrumbs, Block ESP, Waypoints, Trajectories, Logout Spots, Hole ESP, Freecam, Free Look, Xray, Tunnel ESP, Void ESP, Light Overlay |
| World | Nuker, Time Changer, Weather Changer, Timer, Auto Shearer, Auto Nametag, Auto Mount, Flamethrower, Liquid Filler, Spawn Proofer |

Horizon's own auto-reconnect is in Options > Horizon Extras.

Combat modules and No Fall work on servers, single player included (Horizon plays single
player through its own integrated server). ESP, Tracers and Nametags are drawn on the HUD.

## Build it yourself

`python3 tools/pack.py` writes `dist/solstice-<version>.hnmod`. To compile-check against the
pinned Horizon, stage it with [horizon-mixins](https://github.com/horizon-rust/horizon-mixins)
(`horizon-mixins apply --workspace <horizon at the pinned commit> --mods mixins --out stage`)
and run `cargo check -p horizon-client` in the stage.

## Licence

MIT.
