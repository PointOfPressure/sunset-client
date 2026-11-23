# Contributing

Open a pull request on a branch. PointOfPressure reviews and merges them, and does the releases.

By opening a pull request you confirm you wrote the change (or are allowed to give it) and agree that PointOfPressure may use, change and distribute it as part of Sunset under the project's licence or any other.

## What a change can touch

Sunset is a native mod that runs on stock Horizon at the pin in `client/horizon-native.json`. Players install it
through the normal launcher, so a pull request has to work against that pin without changes to Horizon itself.

- Don't patch the engine. No edits to a Horizon checkout or fork, no "add this to your Horizon first" steps.
- Don't add libraries or toolkits that need engine support (imgui, egui or another renderer, a new window or GPU
  context). UI is drawn with what Sunset already uses: `client/sunset/src/game/render.rs`,
  `runtime_gui_draw.rs` and `dashboard.rs` are the places to start.
- Work in Rust inside `client/sunset/` (or `plugins/` for plugin work). Separate runtimes or C/C++ components need
  agreeing in Discord first.
- If a feature really needs a hook Horizon doesn't have, open an issue or ask in Discord instead of patching it in.
  Hook patches live on a maintainer-built fork (`client/horizon-patches/README.md`) and are added there, not in PRs.
- A design you built elsewhere is still welcome: send screenshots or a video and it can be rebuilt on Sunset's UI.

## Working on it

Keep your work in its own files where you can, so changes don't collide. Build with zero warnings.

Code from GPL projects (Meteor, Trouser-Streak and similar) can't be copied or translated; reimplement the behaviour.
Say where an idea came from in the PR so it can be credited.

Run `client/tools/verify.py` if you can (see the README). A change to the plugin API (`client/sunset/src/game/api.rs`)
also needs `plugins/plugsmoke.py`, the release's plugin gate: it builds the SDK's examples and `plugins/plugcheck` into
Sunset and fails unless every `Plugin API:` check passes in game.
