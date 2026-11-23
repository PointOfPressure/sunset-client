# Horizon patch series and LGPL source

Everything in this directory (the patch series, `<pin>/hooks/` and the other patches) changes Horizon and is
LGPL-3.0-or-later like Horizon, not under Sunset's licence: `../release/LICENSE-LGPL-3.0.txt`.

Sunset's prebuilt client (`../scripts/build-prebuilt.sh`) is the pinned Horizon Nightly's own SDK libraries plus two
libraries we compile: `libhorizon_patch.so` (Sunset's mixins and support modules woven into copies of the Horizon
functions they replace) and `libsunset_core.so` (Sunset's `sunset_core` crate). The script builds them with the stock
`horizon-launcher native-build` against the Nightly's published SDK, the same route a player's launcher takes.

## Per pin

- `PointOfPressure/horizon`, branch `sunset/<pin>` (the LGPL corresponding source): upstream `<pin>` plus four commits,
  written by `FORK=<horizon checkout> build-prebuilt.sh`:
  1. `sunset/package/`: the .hmod package (`client/sunset` at the recorded commit; Sunset's licence, not Horizon's).
  2. `sunset/patch/`: the function-replacement crates the launcher extracts from `<pin>`, unmodified.
  3. the same crates with Sunset woven in, plus the launcher's entry module: the source of `libhorizon_patch.so`.
  4. `sunset/README.md`: how to rebuild and what is under which licence.
- `<pin>/` here: commits 3 and 4 as `git format-patch` files (the readable part) and `series` (all four commit ids).
  Commit 1 is `client/sunset` itself and commit 2 is generated (4.5 MB of upstream code), so they live on the fork only.

## Hook patches and the fork SDK

Hooks Horizon lacks are small patches on the pin, branch `sunset-hooks/<pin>` of the fork, one per feature, exported
to `<pin>/hooks/` (`upstream-asks.md` in `../docs/` asks for the same upstream). A `#[replaceable]` below the game
crate only takes effect in a game built with it, so the prebuilt uses an SDK built from that branch:
`FORK=<worktree at sunset-hooks/<pin>> ../scripts/fork-sdk.sh build` (about 1 h cold, the launcher's own Rust), then
`HOOKS=sunset-hooks/<pin> FORK=<horizon checkout> build-prebuilt.sh`, which seeds its launcher home with that SDK and
starts `sunset/<pin>` from the hooks branch. The shipped Horizon libraries are then the fork's build, not the
Nightly's: their source is the same branch.

`2e7d2c6/hooks/` is the same series rebased onto upstream `2e7d2c6` (fork branch `sunset-hooks/2e7d2c6`, head
`9defaa0433`), built through the launcher's developer route rather than as a Nightly's stand-in SDK
(`../docs/hmod-port.md`, "Rebase onto Horizon 2e7d2c6").

Sunset 2.0's Client Spoofer needs the session-aware brand hook from patch `0011`. Build the fork SDK
from a hooks branch that includes this patch before running `native-build` for the updated package.

## LGPL (Horizon is LGPL-3.0-or-later): what we publish and why

- Horizon's libraries ship **dynamically linked** (built from `sunset-hooks/<pin>` when HOOKS is set, unmodified otherwise) (`libhorizon_mod_host.so`, `libmod_*.so`,
  `libhorizon_core.so`, ...; the `horizon-client` launcher stub finds them through `$ORIGIN`). Users can replace them
  with their own builds; that is LGPL-3 section 4(d)(1).
- `libhorizon_patch.so` contains modified Horizon code, so its complete source is published: the fork branch above,
  with the exact launcher, mixins and Rust versions in `sunset/README.md`. Anyone can rebuild it, changed or not.
- Relink kit (`sunset-<version>-<pin>-<platform>-relink.tar.zst`, beside each release): the rlibs, `--extern` lists
  (SDK paths as `$SDK`) and identity files both libraries were linked from, and `relink.sh`, which runs the
  launcher's own link line for `libhorizon_patch.so` and installs the result (`../release/relink/README.md`).
  Limitation: Rust rlibs only link against crates with the same metadata hashes, so relinking against a modified SDK
  needs the same rustc (see `manifest.json` `build.rust`); the route that always works is rebuilding from the fork
  branch with `native-build`.
- LGPL-3 section 4's terms for the combined work (modifying it for your own use, reverse engineering to debug
  that): `../release/PREBUILT-TERMS.md`, shipped in the game folder's `licences/` with the Horizon notice, the
  LGPL and GPL texts and `THIRD-PARTY-LICENSES.txt` (`../release/third-party.sh`), and beside each release with
  `…-horizon-source.tar.zst` (the fork tree) and `…-horizon-series.bundle` (its commits on the pin). The maintainer
  read and signed off this text for 2.0.0 on 20 Nov 2026 at 03:11.

## Other patches

- `launcher-session-hidden.patch`: horizon-launcher side of the hidden-session handling (see `docs/hard-panic.md`).
