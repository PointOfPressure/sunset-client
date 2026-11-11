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
| Watchdog Speed | Ground jump/launch, native airborne motion, input/yaw tracking, knockback-triggered optional boost | Bhop plus Velocity, No Jump Delay and shared direction/ownership | Compare native-air cycle and genuine knockback coordination | Moon's stored speed/input damping is not generally applied to its MoveEvent; do not reproduce dead state | Audit then implement only meaningful distinct behaviour |
| Aura selection/rotation/timing | Separate components; distinct ranges, hurt-time selection, fractional CPS | `aura`, `aura_aim`, `aura_timing`, target/rotation/hand ownership already provide these | No wholesale rewrite justified | Preserve real cooldown/Legacy scheduler and hit feasibility | Already implemented; retain |
| Watchdog AutoBlock | Sword use/release; special 1.12.2 Via use-item packets and alternate old placement packet | Offhand shield release/re-raise in `hands` | Verify modern-to-legacy sword-use capability | Modern shield is not proof of translated sword blocking | Protocol investigation required before exposing Legacy |
| Other scaffold/NoSlow/Velocity/WTap mechanisms | Selected historical packet/tick coordination | Existing Scaffold, No Slow, Velocity and sprint/attack ownership | Evaluate interactions only | Historical packets cannot be injected as modern IDs | No unrelated overhaul |

Moon's NCP potion base uses the zero-based amplifier without adding one. Sunset will not reproduce that Speed I
error. Native attributes and Jump Boost increments govern the client-side adaptation; historical coefficients are
named mode constants, not claimed protocol facts.

## Execution plan and checkpoints

- [x] A: inspect dirty tree, pinned engine, NCP/Watchdog/Watchdog AutoBlock and relevant aura components.
- [x] B: pure NCP controller + explicit phases; measured displacement; input/collision/teleport/world/disable recovery;
  actual attributes, reduced jump setting, shared movement integration. Focused tests, pinned compilation, commit.
- [ ] C: distinct Watchdog ground/native-air cycle and server knockback integration, with Velocity/Sprint/Jump Delay
  ownership. Focused tests, compilation, commit.
- [ ] D: prove translation support or record exact Legacy AutoBlock blocker; reuse/improve supported shield ownership
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

Runtime results: **none for these modes**. No Rewound or pinned historical NCP runtime test has been performed;
use a controlled/pinned server when available. Record server/protocol/config, baseline, accepted displacement,
setbacks, visible violations and disconnects before claiming compatibility. No automatic push.
