# Dependency notices

The vendored horizon-baritone code and its oracle fixtures retain the four
upstream licence/notice artefacts in this directory. See `UPSTREAM.md`.

The independently authored C declarations and transport in
`mixins/baritone/{interface,loader}` are MIT, under `mixins/baritone/LICENSE`.
Only that transport is linked into the game. The engine's manifest does not
depend on Horizon or a game library.

Registry dependencies are pinned by `Cargo.lock`. Their upstream packages
include their licence texts: serde_json, serde/serde_core and itoa use MIT OR
Apache-2.0 terms; memchr uses Unlicense OR MIT and zmij uses MIT. The MIT loader uses libloading 0.8.9 (ISC) and
serde_json 1.0.151 (MIT OR Apache-2.0), resolved through Horizon's existing lock
when staged. `dependency-licenses/INDEX.json` lists all 17 selected package
versions from both the engine lock and Horizon's loader dependency closure;
the adjacent files preserve their exact licence/notice texts, including
Windows support and the lock's optional macro dependencies. Those files ship
with binary and source distributions. Cargo source caches are build inputs,
not release assets. Preserve these notices with redistributed library code.
