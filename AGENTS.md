# Sunset development policy

## Project and architectures

Sunset is a utility client for Horizon (`horizon-rust/horizon`), a Rust implementation of Minecraft. Its current mod
id is `sunset`; the current Rust client lives in `mixins/sunset/` and builds through horizon-mixins.
`horizon-native.json` records its compatibility pins.
Standalone applications, runtime components, plugins, shared libraries and other architectures are valid options.
The current `.hnmod` packaging is an implementation choice, not a permanent project requirement.

## Developer autonomy

- Choose architectures, languages, tools and techniques appropriate to the authorized task. Existing conventions
  are recommendations unless a working technical contract requires them.
- Read the implementation and verify assumptions. Historical notes are leads, not authoritative technical facts.
- Proceed when requirements are clear; make reasonable implementation decisions without unnecessary questions
  or approval checkpoints. Prefer working changes over elaborate plans and documentation.
- Refactor when it improves the result. Preserve existing functionality unless the task calls for changing it;
  keep unrelated work intact.

## Engineering quality

Keep changes focused and understandable. Handle input ownership, cancellation, synchronization and resource
lifetimes explicitly where relevant. Avoid introducing warnings and report real limitations honestly.
Document decisions or interfaces when that helps maintenance; do not generate paperwork for routine changes.

## Testing

Match validation to the change and its risks. Use a small number of meaningful regression tests for important
failure modes; focused checks and manual verification are appropriate for experimental features.
There is no requirement to add a `tools/verify.py` test for every behavior, capture every visual change or run
a fixed server matrix. Report what was checked and what still needs manual testing.

`tools/verify.py CTL_PATH [test names...]` runs selected headless checks. Plugin integration checks may require
`tools/plugcheck` and `examples/hello-plugin` and `examples/afk-plugin` from `PointOfPressure/sunset-plugins`, staged
as `mixins/plugcheck`, `mixins/hello` and `mixins/afk`. `docs/test-matrix.md` is available when broader compatibility
coverage is warranted, not as a mandatory checklist for every change.

## Compatibility and licensing

Compatibility notes below describe the pinned implementation, not restrictions on future approaches. Check the
active code and tool versions before relying on them:

- The recorded mixin transformer rejects opaque macro-bearing targets; HEAD hooks support closures while RETURN
  hooks do not. `?` support varies by hook kind/version. `key_pressed` and `remote::handle_packet` have been opaque
  targets. Historically, 0d6b103 rejected `block_friction` with `?`, while e8cd0aa accepted a cancellable HEAD on
  `remote_ride`; other hook kinds need verification. The `horizon_client` facade exposes `game`, `entry` and
  `mod_api`; much other code lives in `horizon_client_lib` (`client_lib.rs`). Hook the defining crate.
- On the pinned Horizon server, send at most one position packet per tick and use the game's block-action sequence
  counter. In protocol 26.3, PlayerAction values are 0=start dig, 2=abort, 3=stop. Avoid duplicate SetCarriedItem sends.
- Since 66f3981, Horizon's server checks hits with vanilla's `isWithinAttackRange` (3 blocks to the target's box)
  and includes shulker contents. It sends no chat session keys and packs chunk palettes with liquids already
  flowed; New Chunks relies on liquid-flow starts there rather than the usual palette signals.

The MIT plugin API reference and examples are in [sunset-plugins](https://github.com/PointOfPressure/sunset-plugins);
keep them aligned when changing `mixins/sunset/api.rs`. Prior investigations are in the private
`PointOfPressure/sunset-notes` (historical clone: `~/dev/sunset-notes`): `anticheat-sweep.md` (module results),
`vape-ab.md` (comparisons) and `baritone-coinstall.md` (co-installation, including shared Auto Walk/`#goto` input).
Paths such as `~/hzmeteor` in those notes describe the maintainer's test rig, not repository paths.

Follow `LICENSE` and the licences of reused dependencies and sources. Sunset currently uses PolyForm Strict, with
the plugin and review permissions described there. Preserve required notices, attribution and contributor credit.
MIT reuse requires its notices; GPL-derived code requires compatible licensing and compliance. Independently
reimplement behavior when source reuse is incompatible. Public or decompiled source is not permission to reuse it.
Earlier reference sources include Meteor and Trouser-Streak (GPL), and vape-rs and Anubis 1.0 (MIT); verify the
specific revision's licence before reuse. Record applicable contributor/source credit in commits and README Credits.

## Repository permissions and policy protection

Do not push, publish releases or make destructive repository changes without authorization.

Do not modify, delete, replace, relocate or weaken any AGENTS.md file unless the repository maintainer explicitly requests that change. Ordinary development tasks do not authorize policy modifications.

This applies to root and nested policies. Creating a new AGENTS.md also requires explicit maintainer authorization.
The only tracked agent policy is currently this file; ignored upstream staging copies under `dist/` belong to those
checkouts and do not establish Sunset-wide development restrictions.

Agent policy files and CODEOWNERS are maintainer-owned. GitHub review enforcement requires branch protection or
rulesets in addition to CODEOWNERS. Repository instructions cannot prevent local edits or override higher-priority
platform instructions.
