# Feature roadmap

Planning snapshot: 2026-10-31, Sunset main
`fa174f362a640caa34c5f080deadd704dc173c85`. This is a scoped plan for useful
Horizon-native features, with evidence requirements for each contribution.

## Landed contributions

PointOfPressure confirmed these contributions were ported by hand onto the
renamed Sunset tree. The original PRs are closed, rather than GitHub-merged:

- [Radar #1](https://github.com/PointOfPressure/sunset-client/pull/1#issuecomment-6534752406): main commit `46d2da1`.
- [Profiles #2](https://github.com/PointOfPressure/sunset-client/pull/2#issuecomment-6537241326): main commit `713ac5b`, including recovery and export/import.
- [Text editor #4](https://github.com/PointOfPressure/sunset-client/pull/4#issuecomment-6537242984): main commit `959212b`, with Apply/Enter and Cancel/Esc.
- [Mouse and Hold binds #5](https://github.com/PointOfPressure/sunset-client/pull/5#issuecomment-6537805421): main commit `53a7f89`, scoped to modules supporting Hold.

These features should be extended through current Sunset APIs. Old Solstice
branches need rebasing or manual adaptation before review.

## Next focused contributions

1. **Tighten shared scan accounting.** Current `Sweep` already spreads Block,
   Hole, Tunnel, Void and Light scans over ticks and retains nearest results.
   Its nominal 12,000 allowance is checked after whole x slices: a full-height
   range-48 Block scan can account for 18,818 reads in one tick. Independent
   consumers also lack an aggregate/time allowance. Preserve nearest-result
   retention and configurable Max when adding finer cursors and shared limits.
   Compare sparse/dense fixtures and measure frame behavior before claiming gains.
2. **Verify Anti AFK regression coverage.** Upstream subsequently fixed the
   Jump lock recursion in [56b1e36](https://github.com/PointOfPressure/sunset-client/commit/56b1e36858adb5df5008a8ae844d83e9eda2ea12).
   Verify Jump and settings expansion independently against
   [issue #3](https://github.com/PointOfPressure/sunset-client/issues/3); avoid a duplicate fix PR.
3. **Improve HUD control and information.** Start with draft-based placement
   for watermark, module list and coordinates. Add armor durability and optional
   low-health feedback separately; then bound-module state, effects and cooldowns.
   Test input release, resized viewports, F1/F3 and profile persistence. Scaling,
   hiding and additional editor groups remain explicit follow-ups.
4. **Make lists and colors easier to edit.** Searchable selectors need explicit
   item/block/entity metadata, retained unknown IDs and compatible `.set` values.
   RGBA editing needs palette migration, preview and Apply/Cancel semantics.
5. **Add contextual local alerts.** Build deduplicated block discoveries on a
   bounded scanner; follow with nearby-player arrivals, friend filters and
   boundary hysteresis. Keep server join/leave notifications distinct. Weather
   alerts need a policy when Weather Changer overrides the visible weather.

Each code PR should state its exact base, tests, integrated build status and
remaining live checks. Rendering/input behavior needs actual game evidence;
unit tests and compilation alone do not establish appearance or usability.

## Overnight work awaiting current-base integration

The following local prototypes were made on the older Solstice base and are
not claimed as merged or released:

- `hud-group-layout` (`f7c8ba4`), `armor-durability-hud` (`5a1c06a`),
  `low-health-warning` (`d0db40b`) and `registry-picker` (`c06dc09`) had isolated
  old-base link/build checks; final current-base runtime validation remains.
- `keybinds-status-hud` (`d666751`) and `shared-block-hole-budget` (`660577e`)
  had tests/compile checks; a complete linked game build remains unverified.
- `overnight-integration` (`a01204a`) passed a compile check, but its final link
  was interrupted. It has no successful complete build result.

The old Block ESP pilot is superseded by current `Sweep`; applying its fixed
2,000-result cap would regress current retention. The old Anti AFK fix is also
superseded by the upstream commit above. Reconcile prototypes against current
Sunset before review; do not install the old integration over current 0.8.1.

## Reuse existing capabilities

Sunset already supplies broad combat, movement, inventory and rendering
utilities, including dropped-item ESP, movable menu panels, palette colors,
Middle Click Extra and a target HUD. New variants should specify the missing
workflow. Pinned Horizon already supplies auto reconnect, Discord presence and
minimap facilities; reuse them when suitable.

## Later research

- Shield-aware weapon selection and item-use binds need coordinated inventory
  ownership, cooldowns and reliable slot restoration. Extend Auto Weapon and
  Middle Click Extra where possible. Sprint-reset and knockback-jump assistance
  require verified attack timing and multiplayer fixtures.
- Crystal/Anchor Aura and coordinated mining need native damage/exposure
  calculations, placement/action ordering and multiplayer validation.
- Vehicle flight and liquid walking need verified collision/vehicle hooks and
  server behavior. Name protection needs consistent chat/tab/nametag coverage,
  rich-text handling and unchanged outbound identity.
- Activity/chunk finders need bounded scans and measured false positives.
  Model outlines, post-processing and custom skies need original wgpu work.
- DonutSMP region/staff/vanish tools and economy/teleport macros need specified
  server semantics and a suitable test environment. They remain separate work.

## Research credit

Planning used static behavior research from Anubis, whose original-author
metadata names **4ldenz**, alongside Sunset and pinned Horizon source. This
document distributes functional requirements, not Anubis source or assets;
original author and copyright ownership remain with their respective holders.
Keep source attribution for any material actually reused. Feature research does
not imply full feature parity or a complete port.
