# Explosive-combat behavior reference

The reference inspected on 31 October 2026 is the user-supplied **Anubis Client
1.0 (1).jar**, not the older 0.9.9.3 evidence archive. Its `fabric.mod.json`
identifies `1.0+mc1.21.11`, author `4ldenz`, and `GPL-3.0-only`.

Exact JAR SHA-256:
`e60968d360d57e8cfcd68bb127f6f56c7801ddebcff331827102596a209b15f1`.

Static inspection used JDK 21 `javap` and CFR 0.152; critical branches were
checked in both outputs. The JAR was not launched. Decompiled source, binaries
and assets remain outside this repository. These notes describe behavior;
the native modules use Horizon APIs and original Rust implementation.

## Important distinctions in the new build

- **Crystal Aura and Anchor Aura share a planner.** Crystal's coordinator can
  request anchor opportunities through `shots`, `ready` and `fire`; the choice
  considers immediate attacks, target value and self damage. Merely enabling
  two independent loops does not reproduce this behavior.
- **Attack aim and explosion position are separate.** `CrystalAuraModule`'s
  `breakLook` and `hitAlong` validate a ray entering the crystal hitbox. A
  block-level explosion point is not an adequate entity interaction target.
- **Timing is stateful.** Separate place/break clocks, bounded attack retries,
  pending actions, latency-based confirmation and `HurtClock` avoid treating
  every tick as another full-damage opportunity.
- **Damage includes equipment.** `CrystalDamage.Victim` records armor,
  toughness, enchantments, resistance and blocking, and caches exposure.
  Native self-safety must still handle equipment breaking before reduction.
- **Prediction and optional setup extend the core.** `Combo.lead` predicts
  movement with collision/gravity; Crystal Aura can plan obsidian setup and
  lower damage thresholds for face placement. Loot protection is separate.
- **Manual PvP macros are different modules.** `CrystalMacroModule` and
  `SingleAnchorMacroModule` start from player input. The anchor macro offers
  Single/Double, Delay, Safe Anchor, fallback, slot restoration and loot
  protection. They are not equivalent to autonomous Anchor Aura. Its Double
  mode is a conditional second observed sequence, not unlimited repeated use.

The native contribution addresses hitbox aiming, independent crystal clocks,
bounded retries, conservative hurt-window deferral, equipment-aware self
prediction, shared opportunity selection and diagnostics. Separate native
manual macros now implement user-triggered observed transactions. This does
not establish full parity with movement prediction, all manual-macro options, every obsidian setup mode,
face placement or the reference's visual plan renderer. Recent-death protection is now implemented as the narrower policy below.

R11 adds an original optional Auto Obsidian fallback to Crystal Aura. It uses
native support/preview APIs, actual block payload acknowledgement and fresh
post-base scoring. Its conservative fallback-only preference differs from a
planner that promotes a better setup over an already-ready opportunity. This
behavior is opt-in and is not described as exact reference parity.

## Native differences kept deliberately

Friends retain an unmitigated safety bound. No totem-held exception permits a
lethal self hit. Unknown data and exhausted budgets refuse the candidate.
Anchor's tested offhand-glowstone sequence can fill the anchor before using a
non-fuel main hand; the inspected reference instead refuses offhand glowstone.
The native modules wait for actual observed state changes and use bounded
tick work, with no new render effects.

The pinned integrated server re-aims an entity attack at the box center, then
uses a three-block survival ray. An exactly three-block nearest-box contact
can therefore miss under that server's center-directed ray. Test fixtures
stay inside the boundary; public-server behavior is not inferred from this
host-specific observation.

## Original recent-death guard

The exact 1.0 reference sets Protect Loot true in Crystal Aura, Anchor Aura and
Single Anchor Macro. Crystal Macro invokes its shared death latch without a
separate checkbox. Native R10 instead offers one consistent optional setting
across all four owners, true by default. This is an intentional native policy,
not a claim to reproduce the reference's item detection or input latch.

The original Rust guard records observed player death locations for 30 seconds
using native entity identity and last-server positions. It uses fixed storage,
a conservative blast-radius margin, explicit lifecycle boundaries and owned
transaction cancellation. No decompiled Java expression or implementation is
included. See [module scope](combat-modules.md#recent-player-death-protection-r10-source)
for the exact limits and explicit clear command.
