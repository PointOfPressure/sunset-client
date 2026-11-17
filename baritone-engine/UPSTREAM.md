# Upstream provenance

Vendored from [luhnox/horizon-baritone](https://github.com/luhnox/horizon-baritone),
commit `41e1b720dc2028d06618cb8f7cf55f2697bba83c`, version 1.1.52,
on 2026-11-16.

There is one copy of the upstream `src/` tree and `tests/data/` fixtures, in
`mixins/baritone/upstream/`. The game builds it into Sunset's full Baritone
runtime (see `mixins/baritone/horizon-mixin.toml`); this standalone library
compiles upstream's pure module graph from the same files by relative path
(`src/lib.rs`), including its real A*, movement costs, movement executor, goals,
settings parser, caches, schematics, selections and other pure components.
Game-bound upstream modules are not compiled into the engine.

This is a derivative of Baritone 1.21.4 by leijurv and contributors, ported to
Rust by luhnox. Preserve the exact upstream `LICENSE`, `COPYING`, `NOTICE.md`
and `LICENSE-Part-2.jpg` together. Upstream describes its licence as LGPL-3.0
with anime exception; the image and that statement are preserved, not
reinterpreted as another licence grant. `license-file` points to the LGPL text.
See `CHANGES.md` for the integration changes and `DEPENDENCIES.md` for transport
and third-party dependencies.

Sunset's host controller uses a separately built, replaceable engine library. The
MIT companion is independently authored and has no Cargo dependency on this
engine. A compatible modified engine can be selected using the documented
override path without rebuilding the game. Corresponding-source archives must
include this tree, `mixins/baritone/upstream/`, the MIT `mixins/baritone/{loader,interface}` sources, the
loader's MIT licence and `tools/pack.py` (which builds the engine) at their relative paths.
