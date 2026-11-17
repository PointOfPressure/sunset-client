# Contributing

Open a pull request on a branch. PointOfPressure reviews and merges them, and does the releases.

By opening a pull request you confirm you wrote the change (or are allowed to give it) and agree that PointOfPressure may use, change and distribute it as part of Sunset under the project's licence or any other.

Keep your work in its own files where you can, so changes don't collide.

Run `tools/verify.py` if you can (see the README).

Before publishing a release, run the [plugin release check](docs/plugin-release.md). It builds Hello, AFK Fisher
and plugcheck with the host, runs their in-game checks, and packs only after every required check passes.
