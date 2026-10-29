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

Each version is pinned to one Horizon Nightly (`horizon` in `horizon-native.json`) because it
hooks the game's own functions. The launcher builds it on that commit, so it keeps working
after Horizon updates, but plays that Nightly's game until the mod moves to a newer one.

## Use

- **Right Shift** opens the click GUI (rebind it under Options > Mods > Solstice): every
  category as a panel. Left click turns a module on or off, right click opens its settings
  (sliders for numbers, right click a slider to reset it), the Bind row takes the next key.
  Drag the headers to move panels, right click a header to fold it. Esc closes it. The game
  keeps running while it's open.
- The same settings are also pages under Options > Mods > Solstice (`.pages`).
- **Key binds** toggle a module while you play.
- **Chat commands** start with `.` and never reach the server (`..text` sends `.text`):

| Command | Does |
| --- | --- |
| `.help` | list the commands |
| `.menu` | open the menu |
| `.t <module>` | toggle a module (`.toggle`) |
| `.modules` | list every module, `*` on |
| `.settings <module>` | list a module's settings |
| `.set <module> <setting> <value>` | change one (`.set killaura range 4`) |
| `.bind <module> <key\|none>` | bind a key (`r`, `rshift`, `f6`...) |
| `.binds` | list key binds |
| `.reset <module>` | back to defaults |
| `.friend add\|remove\|list [name]` | Kill Aura and Trigger Bot skip friends |
| `.vclip <blocks>`, `.hclip <blocks>` | move up or down, or forward |

Settings and binds are saved in `horizon-mods/.horizon-mod-data/solstice/config`.

## Modules

| Category | Modules |
| --- | --- |
| Combat | Kill Aura, Trigger Bot, Criticals, Auto Totem |
| Movement | Sprint, Auto Walk, Flight, Speed, No Fall, Velocity, Spider, Step, Air Jump, Elytra Boost, Anti Void |
| Player | Fast Place, Fast Break, Auto Respawn |
| Render | HUD, Fullbright, ESP, Tracers, Nametags, Storage ESP |

Combat modules and No Fall work on servers, single player included (Horizon plays single
player through its own integrated server). ESP, Tracers and Nametags are drawn on the HUD.

## Build it yourself

`python3 tools/pack.py` writes `dist/solstice-<version>.hnmod`. To compile-check against the
pinned Horizon, stage it with [horizon-mixins](https://github.com/horizon-rust/horizon-mixins)
(`horizon-mixins apply --workspace <horizon at the pinned commit> --mods mixins --out stage`)
and run `cargo check -p horizon-client` in the stage.

## Licence

MIT.
