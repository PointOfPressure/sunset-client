# Meteor for Horizon

A [Meteor Client](https://github.com/MeteorDevelopment/meteor-client) style utility mod for
[Horizon](https://github.com/horizon-rust/horizon), the Rust rewrite of Minecraft: Java Edition 26.3.
Vibecoded with Claude Code.

Horizon can't run Java mods, so none of Meteor's code is used. This is a new Rust mod: a
Horizon *native mod* (a mixin bundle the launcher compiles into the game), built to work like
Meteor: modules in categories, settings, key binds, a menu and `.` chat commands.
Not made by, or affiliated with, MeteorDevelopment or the Horizon project.

> Using these on servers that don't allow them will get you banned. Play where it's allowed.

## Install

1. Download `meteor-<version>.hnmod` and `install.sh` from [Releases](../../releases).
2. Run `./install.sh meteor-<version>.hnmod`. It copies the file into Horizon's default
   instance (`~/.local/share/horizon/horizon-native-mods/`) and turns it on in `horizon-mods.txt`.
   For another instance, pass its folder as the second argument.
3. In the Horizon launcher: Settings > Game > **Native mods (experimental)** on, then **Build**,
   then "I trust these mods: build". The first build downloads Rust and takes 10 to 30 minutes;
   later builds are a few minutes.
4. Play. The instance now plays the version with Meteor built in.

Horizon's launcher has no button yet for importing a native mod from a file, only from its
registry, so step 2 does that part by hand.

Each version is pinned to one Horizon Nightly (`horizon` in `horizon-native.json`) because it
hooks the game's own functions. The launcher builds it on that commit, so it keeps working
after Horizon updates, but plays that Nightly's game until the mod is moved to a newer one.

## Use

- **Right Shift** opens the menu (rebind it under Options > Mods > Meteor). Categories, then
  modules: each module's page turns it on, sets its key bind and changes its settings.
  Shift-click lowers a number.
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

Settings and binds are saved in `horizon-mods/.horizon-mod-data/meteor/config`.

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

`python3 tools/pack.py` writes `dist/meteor-<version>.hnmod`. To compile-check against the
pinned Horizon, stage it with [horizon-mixins](https://github.com/horizon-rust/horizon-mixins)
(`horizon-mixins apply --workspace <horizon at the pinned commit> --mods mixins --out stage`)
and run `cargo check -p horizon-client` in the stage.

## Licence

MIT.
