# Hard Panic

Panic (the module, its key, `.panic`, or a plugin turning it on) stops Sunset for the rest of the game process.
The next launch starts it as usual; nothing about Panic is saved.

## Lifecycle

`mixins/sunset/lifecycle.rs` (`horizon_protocol::connection::sunset_lifecycle::RUNTIME`, shared by the game,
network and renderer crates) goes `Running → Stopping → Dormant`, one way, with atomic compare-and-swap steps:

- `runtime::shutdown` makes the one `Running → Stopping` step. Every hook checks `running()`, so dispatch closes
  at once. The same call cancels plugin tasks, closes the ClickGUI, takes Sunset's row out of Options > Mods and
  writes the launcher marker below.
- `runtime::finish_shutdown` waits until no plugin callback is still executing (`Lifecycle::enter` counts them), then
  runs `all_off` and the plugins' `on_stop` once and makes the `Stopping → Dormant` step. It runs from the next
  game-thread entry point (key, chat, pre/post tick, plugin setup) when Panic was asked for inside a callback.
- While stopping, a packet the game sends first lets Fake Lag's held packets go (`sunset_lag::drain`), so nothing
  overtakes them; incoming packets Backtrack and Knockback Delay held go out in order at the next `Session::poll`.
- After it the chat is the game's own (no `.` command is read), keys and plugin calls do nothing, and `save` writes
  nothing.

A plugin's `api::stop_everything` is not Panic: it runs `all_off` and the stop handlers, and Sunset keeps running.

## Options > Mods

`ModList::installed` is the Mods page's row list. It is rebuilt only at start (`Game::start_mods`) and is never
saved, so the mod stays built in and on and `horizon-mods.txt` is untouched. Panic removes Sunset's row
(`runtime::unlist`), and `runtime::listed` adds none if the list is made again. The options screen keeps indices
into the rows (its page stack's `Page::NativeMod(i)` and the native key waiting to be bound, private to
`options_ui`), so `mixins/sunset/mod_rows.rs` (a module in `options_ui.rs`) closes a page of the removed row,
back to the list, and moves later rows' indices up. The search box filters options, not mod rows, so it needs
nothing more. `not_built()` adds no row for Sunset either, because the build's `native-mods.json` lists it.

## Launcher

The launcher is another process. The game tells it through a file:
`<game folder>/horizon-session/hidden/<mod id>.<pid>.lock`. The game process creates it after Panic, holds it
under an exclusive OS lock (`File::try_lock`) until the process exits, and never deletes it itself
(`mixins/sunset/session.rs`). The OS releases the lock however the process ends, crash or kill included, so:

- a marker whose lock can be taken belongs to a game that has ended; it hides nothing, and whoever finds it
  removes it (the launcher, or Sunset's next start);
- the launcher's folder is the instance's `home`, where it starts the game (`HORIZON_HOME`, current directory),
  so other instances are not affected;
- a launcher restarted while the game runs reads the same marker again;
- PIDs are only in the name to keep two game processes of one instance apart. A reused PID changes nothing,
  because the lock is the proof the process is alive.

The launcher side is [horizon-patches/launcher-session-hidden.patch](horizon-patches/launcher-session-hidden.patch),
made against Horizon 66f3981 (`launcher/src/native_status.rs`, `launcher/src/instance_mods.rs`). It is not part of
Sunset's build: horizon-mixins patches the game's crates only, so the patch has to land in Horizon's launcher (an
upstream change to `horizon-rust/horizon`) or in a locally built launcher. It adds `native_status::hidden(home)`,
puts the hidden ids in `native_status::fingerprint` (read every frame, so the Mods tab reads its list again when a
game hides a mod or exits), and leaves those rows out in `instance_mods::read`. Nothing installed, built or
recorded changes. Without the patch, the launcher lists Sunset as before; the in-game side does not depend on it.
