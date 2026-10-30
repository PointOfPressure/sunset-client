# Sunset plugin API (version 1)

A Sunset plugin is another [Horizon native mod](https://github.com/horizon-rust/horizon/blob/main/docs/native-mods.md)
that adds modules to Sunset's click GUI. It is built into the game together with Sunset, so it
needs no access to Sunset's source: it uses a small module Sunset provides, `sunset_api`.

A whole working plugin is in [`examples/hello-plugin`](../examples/hello-plugin).

## What a plugin is

Three files in a folder `mixins/<id>/`:

- `horizon-mixin.toml`: adds your code as a module of the game's source, and hooks Horizon's own
  `native_tick` (and `native_hud`, `native_key` if you want them) to call it.
- `plugin.rs`: your code.
- `tick.rs`: a one-line helper that calls `plugin.rs`.

It must name the same `horizon_revision` as the Sunset it plugs into (Sunset's own
`horizon-native.json` says which). Put `sunset.hnmod` and your plugin's `.hnmod` in
`horizon-native-mods/` and the launcher builds them together.

## The API

In your module, `use super::sunset_api as api;`.

| | |
| --- | --- |
| `api::API_VERSION` | `1`. A new version only adds things. |
| `api::register(module)` | Adds a module to the GUI, the `.modules` list and the config. Safe to call every tick: a name already there is left alone. |
| `api::module("Name")` | The module's current settings while the player has it **on**, else `None`. |
| `api::chat(game, "text")` | A `[Sunset]` line in chat. |
| `api::turn_off(game, "Name")` | Turns a module off. |
| `api::Module::new(name, category, about)` | Builds a module; then `.bool(name, default)`, `.num(name, default, min, max, step)`, `.pick(name, &[choices])`, `.text(name, default)` add settings. |
| `m.b("x")`, `m.n("x")`, `m.p("x")`, `m.t("x")` | Read a bool, number, choice or text setting. |
| `api::Category` | `Combat`, `Movement`, `Player`, `Render`, `World`. |

Helpers Meteor's own modules use, so a plugin can place blocks and pick targets the same way:

`place_block(game, pos, hotbar_slot, rotate, check_entities)`, `can_place`, `replaceable`, `is_air`,
`facts` (a block's properties), `find_hotbar`, `find_inventory`, `in_reach`, `clear_line`, `listed` (is a
name in a comma list), `player_target`, `other_players`, `every_living`, `PRIORITIES`, `show(pos, argb)`
(a box drawn this frame), `hold(game, slot)`, `look_at`, `dist`, `center_of`, `sequence(game)`.

Anything else of the game's is reachable from your module (it sits inside the game's source): `game.inventory`,
`game.player`, `game.mobs`, `game.send(packet)`, and so on. Those are Horizon's internals and can change between
Nightlies, which is why a plugin names a Horizon commit.

## Rules that keep a server from kicking you

- **One position packet a tick.** Horizon's server (like vanilla's) disconnects a client that sends a second
  `MovePlayerPos` in one tick. Let the game send the tick's own packet.
- **Block-action sequence numbers must never go down.** Use `api::sequence(game)`, never your own counter.
- Don't hook a function that contains a macro (`vec!`, `format!`, `log::...`): the mixin tool refuses it.

## Example

`examples/hello-plugin/mixins/hello/plugin.rs`:

```rust
use super::Game;
use super::sunset_api as api;

pub(crate) fn tick(game: &mut Game<'_>) {
    api::register(
        api::Module::new("Hello", api::Category::Player, "Says hello in chat now and then.")
            .num("Every (seconds)", 10.0, 1.0, 60.0, 1.0)
            .text("Message", "Hello from a plugin"),
    );
    let Some(m) = api::module("Hello") else { return };
    if game.ticks % (m.n("Every (seconds)") as u64 * 20) == 0 {
        api::chat(game, m.t("Message"));
    }
}
```

Build it into the game: `horizon-launcher native-build --trust-native-code` (or the launcher's Build button)
with both `.hnmod` files installed. To check it compiles without the launcher, stage both folders with
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) and run `cargo check -p horizon-client`.
