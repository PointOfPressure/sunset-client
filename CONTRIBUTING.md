# Contributing

Open a pull request on a branch. PointOfPressure reviews and merges them, and does the releases.

By opening a pull request you confirm you wrote the change (or are allowed to give it) and agree that PointOfPressure may use, change and distribute it as part of Sunset under the project's licence or any other.

Keep your work in its own files where you can, so changes don't collide.

Run `client/tools/verify.py` if you can (see the README). A change to the plugin API (`client/sunset/src/game/api.rs`)
also needs `plugins/plugsmoke.py`, the release's plugin gate: it builds the SDK's examples and `plugins/plugcheck` into
Sunset and fails unless every `Plugin API:` check passes in game.
