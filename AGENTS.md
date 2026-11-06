# Notes for agents working on Sunset

Sunset is a Meteor-style client for Horizon (horizon-rust/horizon, a Rust rewrite of Minecraft 26.3), built as a
horizon-mixins native mod. Mod id `sunset`; all code is in `mixins/sunset/`. The Horizon nightly it pins is in
`horizon-native.json` (currently 66f3981, horizon-mixins e8cd0aa); check engine code against that revision, not upstream main.

## Hooking limits (horizon-mixins)

- A hook target that uses any macro (`vec!`, `format!`, `log::`...) is refused. Hook a different function.
- Closures are fine in HEAD hooks, refused in RETURN hooks.
- `key_pressed` and `remote::handle_packet` can't be hooked (opaque macros).
- Functions with `?` in them (e.g. `block_friction`) couldn't be hooked on 0d6b103; e8cd0aa takes a cancellable HEAD
  on one (`remote_ride`). Other kinds on `?` functions are untested.
- `horizon_client::` names only the `game`, `entry` and `mod_api` modules now; the rest of the client is in
  `horizon_client_lib::` (same folder, `client_lib.rs`). Hook targets must use the crate that defines them.

## Protocol rules

- One position packet per tick. A second one in the same tick gets you kicked.
- Use the game's own sequence counter for block actions; never keep your own.
- 26.3 PlayerAction: 0 = start dig, 2 = abort, 3 = stop.
- Send a held slot change (SetCarriedItem) once; sending it twice flags Grim (BadPacketsA).

## Horizon's own server

- It checks a hit with vanilla's attack range (`isWithinAttackRange`, 3 blocks to the target's box) since 66f3981;
  before that it re-ran every hit along your look.
- It sends shulker boxes with their contents since 66f3981 (before: without).
- It never sends chat session keys.
- It packs chunk palettes and pre-flows liquids, so New Chunks can only go by liquid flow starts there.

## Rules for changes

- Zero warnings. No generated docs, no AI or SPDX headers in source.
- Every new module or behaviour gets a test in `tools/verify.py` (`@test("Name")`).
- GPL code (Meteor, Trouser-Streak) is behaviour only: write it fresh, never translate line by line.
  MIT sources (vape-rs, Anubis 1.0) may be ported closely; credit them in the commit. No third-party notices
  section in LICENSE; don't put the vape-rs/Anubis attributions back.
- Credit contributors in the commit and in README's Credits.
- Don't hide Sunset from Horizon's mod menu. Client Spoofer is allowed: it spoofs the client brand and the client
  identity sent to servers (brand spoofing included). Nothing else that hides Sunset from servers.
- Don't change existing Kill Aura behaviour; new modes go alongside.

## Testing

- Menu or visual change: build, then one single-player screenshot.
- Gameplay or packet change: single player and Paper while working.
- End of a batch: single player, Horizon's dedicated server, vanilla and Paper.
- Before a release: the six-server matrix (`docs/test-matrix.md`).
- `tools/verify.py CTL_PATH [test names...]` runs tests in a headless game through horizon-ctl.
- The plugin API tests (`Plugin API: ...`, `AFK Fisher: ...`) need `tools/plugcheck` and sunset-plugins'
  `examples/hello-plugin` and `examples/afk-plugin` staged next to Sunset (mixins/plugcheck, mixins/hello, mixins/afk).

## Notes from earlier runs

Kept in the private repo PointOfPressure/sunset-notes (clone: `~/dev/sunset-notes`), not here. Write new run notes
there, never in this repo.

- `anticheat-sweep.md`: every module against Grim, Intave, Spartan, NCP and Horizon's anticheat
  (Grim status list).
- `vape-ab.md`: vape-rs against Sunset on the same anticheats, verdict per module.
- `baritone-coinstall.md`: horizon-baritone installed beside Sunset (works; Auto Walk and `#goto`
  both hold W).

Plugin API docs and the example plugin live in the public PointOfPressure/sunset-plugins (MIT); keep it in step
when `mixins/sunset/api.rs` changes.

Paths like `~/hzmeteor` in those notes are the maintainer's test rig, not part of this repo.
