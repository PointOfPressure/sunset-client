# Baritone for Horizon

A Rust port of [Baritone](https://github.com/cabaletta/baritone) (1.21.4) for
[Horizon](https://github.com/horizon-rust/horizon), the Rust rewrite of Minecraft Java Edition 26.3. It
is a Horizon **native mod**: a bundle of Rust source that `horizon-mixins` splices into the game before
it is compiled. The Java mod cannot run in Horizon, so this is written again against Horizon's own
world, collision and input code.

> **Status: close to complete.** [PARITY.md](PARITY.md) lists every class of Baritone 1.21.4 and says which are
> written from the Java, which are simpler, and which are missing. Every setting, command, process and behavior
> is here, `logAsToast` and the beacon beam of `renderGoalXZBeacon` included (the game's own toast and beam; with
> `renderGoalIgnoreDepth` the beam is scaled toward the camera so it shows through terrain). What is not done: the cached
> chunks drawn semitransparent (`renderCachedChunks`), because Horizon gives a mod no renderer of its own.
>
> One deliberate difference from the Java: `#mine` and `#goto <block>` of a block that is not found underground (logs, sand,
> flowers, crops, dirt) walk the surface looking for it instead of digging down to the strip-mining height, which is where the Java goes
> whatever the block is. Ores, ancient debris, the deepslate layer and the like still branch underground as in the Java.

Pinned to Horizon commit `6475f62f3c834b7489e1816bf47f47ea5eb2b879` (Nightly 2026-11-04 `6475f62`, the newest), the same one SeedCrackerX is pinned to, so the two build together. Only the newest Nightly is kept up to date; releases before 1.1.50 are for older Nightlies and say which in their title. The mod reads the
game's private state, so a later Nightly may need it ported.

## Using it

Commands are typed in chat with Baritone's prefix, `#`, and never reach the server. `#help` lists them
(click a line for its help), `#baritone` is `#set modified`, `#set list` lists the settings, and TAB
completes commands, settings, block names and properties, files, players and waypoints. Baritone keeps going with the chat or a menu
open. It works on multiplayer servers as the Java does, and most servers ban automation: that choice is yours.

The settings you changed are kept in `horizon-mods/.horizon-mod-data/baritone/settings.txt`, one `name value`
per line as in the Java's `baritone/settings.txt`. Edit it by hand and `#set load` reads it back.

The command names, aliases, descriptions and messages are Baritone 1.21.4's (`baritone.command.defaults`).

## Install

A mod is built into the Horizon commit it is pinned to. Only the newest Nightly is supported, so there is one download: the
release is titled `Baritone <version> for Nightly <date> (<7 letters>)` and its file is named
`baritone-<version>-nightly-<date>-<7 letters>.hnmod`, the date and the 7 letters the launcher shows in its Versions tab.

| Nightly in the launcher | Baritone | Download |
|---|---|---|
| 2026-11-04 `6475f62` (up to date) | the latest | [latest release](https://github.com/luhnox/horizon-baritone/releases/latest), file `baritone-<version>-nightly-2026-11-04-6475f62.hnmod` |

Install the file in the launcher's native mods and use the same Nightly for the game, so the launcher builds on the source it
already has. The build takes 10 to 30 minutes the first time. It runs native code with your rights: read the source first. A mod
pinned to another Nightly makes the launcher ask before it builds; a mod whose hooks no longer exist (AppleSkin, Eating
Animation and Sunset on 2026-11-04) stops the build, so turn those off.

```
horizon-launcher native-build --trust-native-code
```

A `release` build (`horizon-launcher settings native-profile release`) is recommended: the `dev` profile makes the
game's start function too large for the default 2 MB stack (`tools/patch_stack.py` raises it for a `fast` build).
To make the file yourself, `python tools/pack.py` writes `dist/baritone.hnmod`.

### Signing key

The versions in the Horizon registry (horizon-rust/horizon-mods) are signed with a [minisign](https://jedisct1.github.io/minisign/) key
of the author of this port (luhnox). Since 2026-11-05 the key is

```
RWTHPH49xk0RxlR1W1RZwYeuLJjnGPW0U/nElRyWuiDh6S1zz9ey1QTj
```

(`minisign -Vm <file>.hnmod -P <the key>` checks a file). It replaces the key of 1.1.38, whose secret half was lost; the
registry entry is changed by a pull request opened from the GitHub account `luhnox`, the owner the entry names, and this paragraph,
committed to the repository the entry names as its source, is the second place the new key is recorded.

## Layout

```
horizon-mixin.toml      what is spliced where: modules (src/) and hook points (hooks/)
horizon-native.json     the registry entry of the .hnmod
src/
  baritone.rs           Baritone and Core: the instance, the tick, the chat, the pathing control
  api/                  goals, settings, utils            (baritone.api.*)
  pathing/              A* search, move generation, costs, the path executor
  behavior/             pathing, look (free look), input, inventory   (baritone.behavior.*)
  process/              mine, follow, farm, explore, build, elytra, ...   (baritone.process.*)
  command/              the commands and their argument parser   (baritone.command.*)
  cache/  selection/  schematic/                          (baritone.cache, .selection, .utils.schematic)
  game/                 what touches the game: the world view, drawing, the bridge into its modules
  tests.rs              tests of the parts that use no game types, run by Sunset's engine crate (../../../baritone-engine)
hooks/                  one small function per hook point (chat, tick, keys, tab completion, lines)
tools/                  pack.py (builds the .hnmod), dev-sync.sh (applies it to a Horizon checkout), patch_stack.py
dist/                   where pack.py writes baritone.hnmod (not tracked; releases carry it)
PARITY.md               the class-by-class comparison with Baritone 1.21.4
```

`#[allow(dead_code)]` marks what the Java's API has and nothing in this mod calls, kept so that the API stays whole.

Inside the game every file of `src/` becomes a module of `game.rs` named `baritone_<file>` (see
`horizon-mixin.toml`), so `use super::baritone_goals::...` reads the same in the game and in the engine
crate. The `hooks/` files are not modules: each holds exactly one function that the mixin tool inserts at
the start of a game function.

## Working on it

```
cargo test --release        # in ../../../baritone-engine: the pure parts (goals, costs, A*, settings, selections, ...)
python tools/pack.py        # rebuild dist/baritone.hnmod from horizon-mixin.toml
```

To check against the game itself, `tools/dev-sync.sh` applies the bundle to a copy of the pinned Horizon
source (it needs `cargo` on `PATH`, `HORIZON_WORK` and `HORIZON_LAUNCHER`), and a normal
`cargo build -p horizon-client --features horizon-client/mods` in that copy builds a game to try it in.

Rules of this code, learned the hard way on Horizon:

- A hook function holds one function and nothing else; keep it short and put the work in `src/`.
- A panic ends the whole game, so fallible calls return `Result` and the commands answer with a
  message instead of crashing.
- The instance is taken out of its cell for the length of a call (`baritone::with`); never keep a
  borrow of it across a call back into the game.
- Blocks are `Pos = [i32; 3]`; the player's own position is `[f64; 3]`.

## License

Baritone is LGPL-3.0 (with an anime exception, see its [LICENSE-Part-2.jpg](LICENSE-Part-2.jpg)) and this is a derivative of it,
so it is LGPL-3.0 as well: [LICENSE](LICENSE), [COPYING](COPYING) and [NOTICE.md](NOTICE.md).

## Changes in Sunset

This copy is horizon-baritone `41e1b72` as built into Sunset (Rust, zackmsa777-a11y, and the Sunset contributors,
November 2026). It runs under the mod id `sunset_baritone`, shares movement with Sunset (stop, Panic, hand-off) and
leaves idle ticks to Sunset. Changed files:
`src/baritone.rs`, `src/behavior/input.rs`, `src/cache/cached_world.rs`, `src/game/bridge.rs`,
`src/pathing/calc/a_star.rs`, `src/process/elytra/flight.rs`, `src/process/follow.rs`, `hooks/block_change.rs`,
`hooks/block_interact.rs`, `hooks/chat.rs`, `hooks/click_escape.rs`, `hooks/click_key.rs`, `hooks/click_press.rs`,
`hooks/click.rs`, `hooks/command.rs`, `hooks/goal_beam.rs`, `hooks/keys_before.rs`, `hooks/keys_remote.rs`,
`hooks/list.rs`, `hooks/page_drag.rs`, `hooks/page_draw.rs`, `hooks/page_key.rs`, `hooks/page_mouse.rs`,
`hooks/page_text.rs`, `hooks/page_wheel.rs`, `hooks/settings.rs`, `hooks/tick.rs`, `hooks/toast_draw.rs`.
Also, in 23 files the items that take the game's private `Game` type are `pub(super)` instead of `pub`, and
parts only the standalone crate uses are marked `#[allow(dead_code)]`, so Sunset's build has no warnings.
Upstream's standalone `Cargo.toml`, `Cargo.lock` and `src/lib.rs` are left out: Sunset's engine crate,
`baritone-engine/`, builds and tests the game-free parts from these same files.
The rest is unchanged. It stays LGPL-3.0: see LICENSE, COPYING and NOTICE.md.
