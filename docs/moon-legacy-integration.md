# Moon legacy PvP integration

Goal: independently implement useful legacy movement/combat behaviour in Sunset, preserving existing modes.
Architecture: pure per-mode movement controllers feed the existing movement override; combat continues to use
Sunset's scheduler, rotation and hand ownership. No legacy packet IDs or Moon source are transplanted.
Stack: Rust, current Horizon/mixin pins; no new dependencies. All new modes are **Experimental**.

## Sources and compatibility

Reference: [StylesFoundation/Moon](https://github.com/StylesFoundation/Moon), revision
`55c89cd4fd3f509db0a2116bb3719d8296eadcf2`. Root contains no licence grant; source was read as a behavioural reference
only. Selected Java files are cached outside this repository. Existing Inventory Manager working changes are outside
this task and must not enter its commits.

Horizon: `66f3981f522b24c250b21523a70080965b940424`, protocol/game 26.3; transformer
`e8cd0aa38cffef49cb2e088ef716baa107148c02`. Modern collision/item-use semantics remain in force. Legacy combat
scheduling is not protocol translation. Rewound's modified NCP revision/configuration is not known or tested here.

## Comparison / decisions

| Feature | Moon implementation | Sunset equivalent | Missing behaviour | Compatibility | Decision |
| --- | --- | --- | --- | --- | --- |
| NCP Speed | Ground launch, measured-distance transition, distinct potion coefficients, recurrent airborne speed | Strafe has different launch/transition/decay; Bhop uses configurable carried speed; Low Hop/Custom are different | This specific phase controller, lifecycle recovery | Modern physics still apply; use actual movement/jump attributes and correct effect levels | Valuable new Experimental NCP mode |
| Watchdog Speed | Ground jump/launch, native airborne motion, input/yaw tracking, knockback-triggered optional boost | Bhop plus Velocity, No Jump Delay and shared direction/ownership | Native-air cycle and genuine knockback coordination | Moon's stored speed/input damping is not generally applied to its MoveEvent; do not reproduce dead state | Added distinct Experimental controller |
| Aura selection/rotation/timing | Separate components; distinct ranges, hurt-time selection, fractional CPS | `aura`, `aura_aim`, `aura_timing`, target/rotation/hand ownership already provide these | No wholesale rewrite justified | Preserve real cooldown/Legacy scheduler and hit feasibility | Already implemented; retain |
| Watchdog AutoBlock | Sword use/release; special 1.12.2 Via use-item packets and alternate old placement packet | Offhand shield release/re-raise in `hands` | Owned translated sword use, attack release and reblock | Standard main-hand use is representable through ViaVersion; complete Rewound chain unverified | Added Experimental Legacy; Modern reuses shield controller |
| Other scaffold/NoSlow/Velocity/WTap mechanisms | Selected historical packet/tick coordination | Existing Scaffold, No Slow, Velocity and sprint/attack ownership | Evaluate interactions only | Historical packets cannot be injected as modern IDs | No unrelated overhaul |

Related findings (all listed reference files were inspected):

| Mechanism | Actual Moon behaviour | Sunset comparison / decision |
| --- | --- | --- |
| `HypixelSprint` | Ground speed caps depend on potion/protocol; alternates shifted movement positions and queues placement/animation; correction cooldown | Existing Scaffold owns movement correction and placement. Skip this packet manipulation; new Speed modes yield to Scaffold. |
| `HypixelTower` | Small vertical launch, stops vertical motion at rounded integer height, fixed forward direction/speed while towering | Existing tower controller owns that state. No new tower; fixed old collision/height assumptions are not established on 26.3. |
| `NCPNoSlowDown` | Releases before movement and reuses item afterward, unless Aura owns a target | Existing No Slow percentages/Skip/GrimAC remain. Owned AutoBlock release bypasses intentional No-item-release suppression; no competing use loop. |
| `UpdatedNCPNoSlowDown` | Temporary held-slot changes for use; queues movement while blocking, flushes with release/re-use | Skip queued replay and slot churn; would compete with existing movement/hand ownership. |
| `WatchdogVelocity` | Records upward knockback, sets its event flag, applies a downward impulse after three updates only with Speed off | Existing Velocity scaling remains authoritative. No copied velocity mode; Watchdog observes only genuine motion packets and their actual resolved Velocity share. |
| `BlatantWTap` | Wraps attack with stop/start sprint commands, optionally twice | Existing WTap/sprint coordination retained. Shared AutoBlock release precedes the accepted attack independently of those commands. |

Moon's NCP potion base uses the zero-based amplifier without adding one. Sunset will not reproduce that Speed I
error. Native attributes and Jump Boost increments govern the client-side adaptation; historical coefficients are
named mode constants, not claimed protocol facts.

## Execution plan and checkpoints

- [x] A: inspect dirty tree, pinned engine, NCP/Watchdog/Watchdog AutoBlock and relevant aura components.
- [x] B: pure NCP controller + explicit phases; measured displacement; input/collision/teleport/world/disable recovery;
  actual attributes, reduced jump setting, shared movement integration. Focused tests, pinned compilation, commit.
- [x] C: distinct Watchdog ground/native-air cycle and server knockback integration, with Velocity/Sprint/Jump Delay
  ownership. Focused tests, compilation, commit.
- [x] D: establish a representable translation mechanism and document endpoint uncertainty; reuse/improve shield ownership
  and attack coordination where justified. Focused tests, compilation, commit.
- [ ] E: focused integration checks, independent final review, recorded commit hashes and runtime results.

Validation: pure state/controller tests first (launch, measured transition, effects, yaw wrap, collisions, resets,
external velocity); pinned mixin application and `cargo check -p horizon-client --tests`. Runtime checks are reported
separately. No client-only speed or absence of local errors is evidence of server acceptance.

Phase B validation: ten pure movement/controller tests passed (four NCP regressions plus six existing movement
tests); pinned mixin application and `cargo check -p horizon-client --tests` passed. Ballistic jump flag consumption
and reset tests compile with the native tests. Existing Strafe/Bhop/Smooth/Low Hop/Custom paths retain their formulas.
NCP yields to Scaffold, Flight, inventory freeze, item-use policy and knockback pause. Native jump power handles
registered block factors and Jump Boost; only the jump-strength portion is reduced by the optional setting.

Phase B commit: `b819153`.
Phase C commit: `7d5bde4`.
Phase C validation: fourteen pure controller/shared movement tests passed; pinned apply and native test compilation
passed. Watchdog launches through native jump power and writes its launch velocity once; ordinary airborne ticks
retain native movement. `Watchdog knockback boost` is off by default and observes only this player's received
`SetEntityMotion`, after Velocity's configured scaling/pause. It steers at most 25 degrees once within six ticks,
preserving the actual impulse magnitude. It does not synthesize Moon's configured damage multiplier, treat
explosions as hit boosts, or restore removed velocity. Disable/release removes only owned surplus launch momentum.
Both controllers yield to Scaffold rather than compete with its tower/movement correction. Ten recovery ticks
follow a correction; three corrections less than 100 ticks apart turn Speed off. These are safety bounds, not NCP rules.

## AutoBlock protocol result and behaviour

[ViaVersion's world packet rewriter](https://github.com/ViaVersion/ViaVersion/blob/22ea5afea51b01ec3e1d3f6d94cdf173d3dd3947/common/src/main/java/com/viaversion/viaversion/protocols/v1_8to1_9/rewriter/WorldPacketRewriter1_9.java)
maps main-hand Use Item to legacy in-air item use with the held item. Its fake-shield branch is restricted to older
clients; it is not required by this implementation. This establishes a representable translation mechanism, **not**
Rewound's end-to-end protocol-777 support or server acceptance. Rewound's proxy/translator revision is unknown.

`AutoBlock: Off / Legacy / Modern` is added to KillAura. Old `Shield=true/false` configuration/commands still map to
Modern/Off; explicit new AutoBlock profile values override that alias. Modern reuses the offhand shield controller.
Legacy requires Legacy combat scheduling (explicitly or Auto's no-cooldown evidence), a held sword, a live valid
Aura target, a synchronized held slot and available shared hand. It sends normal typed protocol UseItem with the
native sequence counter. It never writes a 1.8 packet ID, fakes a shield or sets native `game.using` for a sword.

Legacy holds one owned use request. Actual outgoing attacks release it immediately beforehand; `Legacy reblock
delay` controls restoration, with no release/re-use spam on ordinary idle ticks. Slot packets, foreign uses,
target loss, GUI/input interruption, mode/module disable, death, scene change and Panic retire its ownership.
Modern shield ownership is attributed to KillAura or Trigger Bot so disabling one cannot release the other's use.
Normal translated blocking supplies 20% input and stops sprinting; existing No Slow percentages may override that.
GrimAC's modern No Slow technique is not assumed to work for translated swords, so it keeps normal slowdown.

Moon's special 1.12.2 hand-alternating Watchdog packets, damage multipliers unrelated to actual retained knockback,
double-click bursts, cosmetic fake sword use and queued movement replay were deliberately omitted. Horizon still
renders modern sword mechanics; there is no asserted local sword-block animation or server blocking acknowledgement.
`.auradebug` reports a Legacy use **request (unconfirmed)** rather than claiming effective damage reduction.

Phase D validation: 25 pure tests passed, including release/reblock ownership and composition with the unchanged
Legacy CPS scheduler. Pinned native mixin application and `cargo check -p horizon-client --tests` passed. Native
profile-migration and motion-flag tests were compiled, not executed. No live server or GPU/gameplay test was run.

## Focused integration evidence / manual work

Independent source review found and resolved zero-share knockback steering of old momentum, interrupted Watchdog
cleanup ownership, Modern shield cleanup on death/scene changes and cross-module shield ownership. No new packet
queues or movement packets were introduced. Actual server acceptance still requires testing.

| Case | Automated/static evidence | Runtime result |
| --- | --- | --- |
| NCP alone / Sprint / effects | Launch, actual-distance transition, recurrence, direction, collision/reset tests; native attributes/jump helper compile | Not run |
| Watchdog alone / Sprint / No Jump Delay | Separate launch/native-air state tests; mode-owned jump requests, no extra held key/controller | Not run |
| Speed + Velocity | Genuine motion provenance, zero/partial-share handling, capped wrapped steering and external ownership tests | Not run |
| Speed + KillAura / Criticals / Scaffold | Shared movement infrastructure; legacy modes yield during active Criticals and to Scaffold/Flight/inventory ownership | Not run |
| Aura + AutoBlock + WTap/Criticals | Pure release/reblock tests; accepted outgoing Attack prefix handles all sources; scheduler unchanged | Not run |
| AutoBlock + weapon switching | Slot synchronization; foreign slot/use invalidates owned use without later releasing another owner | Not run |
| Disable / reconnect / Panic | Controller resets, scoped shield ownership, same-scene cleanup versus new-scene retirement; native test compilation | Not run |

Manual procedure: in an isolated controlled server inventory, compare normal movement first, then each new mode;
repeat with Sprint, Velocity zero/partial shares, Jump Boost/Speed, direction reversals, collisions, GUI interruptions,
Scaffold and active Aura/Criticals. For AutoBlock select Legacy combat plus Legacy AutoBlock only on a verified
legacy translation endpoint; log outgoing use/release/slot/attack order and verify damage reduction on the server.
Test target loss, weapon changes, manual item use, old Shield profiles, Trigger Bot ownership, death, reconnect and
Panic. Record authoritative displacement/corrections/violations/disconnects; absence of a client error is insufficient.

Runtime results: **none for these modes**. No Rewound or pinned historical NCP runtime test has been performed;
use a controlled/pinned server when available. Record server/protocol/config, baseline, accepted displacement,
setbacks, visible violations and disconnects before claiming compatibility. No automatic push.
