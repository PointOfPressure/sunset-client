# Mace modules in Meteor Client and its addons: research for Auto Weapon (Mace)

Research notes for redesigning Sunset's Auto Weapon **Mace** section. The document has three parts:

1. How Meteor Client, some Meteor addons and a few other open-source clients handle the mace.
2. How those techniques fare against GrimAC, from Grim's own source and issue tracker.
3. The agreed plan for the new module and how to prove it on a Grim server.

Written 2026-11-03. All third-party code was read statically. Nothing was built or run.

## Ground rules for whoever picks this up

- **Ideas only, no copied code.** The exception is Nora-Tweaks, which is CC0.
- **Licences:**
  - GPL-3.0: Meteor, LiquidBounce, BlackOut-CE and Aoba.
  - No licence found: Trouser-Streak (GitHub doesn't detect one) and PVPEnhanceAddon.
- **Never claim a mode bypasses an anticheat** (Grim or any other) without reproducible, authorised testing.
- **Don't use leaked, cracked or "skid" repos.** A GitHub code search for "MaceKill" or "AutoMace" turns up many of
  them: Open*, *-Crk, *skids and copies of closed clients. All of them were skipped here.
- **Line numbers drift.** Find Sunset's code by function name; the line numbers below are from main `cb6f6b3`.

## Vanilla mechanics that matter

- **Smash attack:** the attacker has fallen more than 1.5 blocks and isn't gliding.
- **Bonus damage:** 4 per block for the first 3 blocks, 2 per block up to 8, then 1 per block. Sunset implements this
  as `smash_bonus` in `mixins/sunset/melee_policy.rs`.
- **Density:** adds `smash_damage_per_fallen_block`, which is 0.5 per level per block fallen.
- **Breach:** lowers how much the target's armour protects, 15 % per level.
- **Wind Burst:** a smash hit launches the attacker upward. You get a new fall without touching the ground, which is
  what makes chains possible.
- **Shields** block the mace. An axe hit disables a raised shield. "Stun slam" (axe, then mace in the same tick) uses
  this.
- **The server judges a hit with the item held when the attack packet arrives.** Swapping, attacking and swapping
  back in the same tick uses the swapped item. Meteor calls this an attribute swap.
- **Vanilla's "moved too quickly" check** allows more distance the more move packets arrive in a tick, up to
  5 × 100 squared blocks, which is about 22 blocks. This is why every packet MaceKill defaults to a 22-block fake
  fall. Paper instead allows about 10 blocks per packet. This is our reading of the server code; it has not been
  tested.

## Sunset today (main `cb6f6b3`)

Auto Weapon is defined in `mixins/sunset/melee.rs`, in `defaults()`. Its Mace settings are:

| Setting | Default | What it does |
|---|---|---|
| Mace | on | Section switch |
| Mace swap | on | Swaps to the mace for a hit |
| Mace pick | Damage / Wind Burst | Which mace to take |
| Breach swap | off | Use a Breach mace outside a smash |
| Smash assist | off | Times the smash for you |
| Smash timing | Delayed / Direct | How Smash assist times it |
| Min fall | 3.0 (1.5–30) | Fall needed before a smash |
| Smash mobs | off | Smash mobs too, not just players |

Under Axe on shields there is also **Stun slam**. Visibility rules are in `runtime.rs`, in the `"Auto Weapon"` arm,
around line 388.

| Piece | Where | What it does |
|---|---|---|
| `best_mace` | melee.rs | Takes the hotbar mace with the highest real damage estimate (`estimate_weapon`: the game's enchantment value effects, vanilla smash bonus, target armour). With Pick on Wind Burst, a Wind Burst mace comes first. Outside a smash only Breach maces count. |
| `preferred_weapon` | melee.rs | The order is: axe at a raised shield, then mace (any smash, or Breach doing ≥ 15 % more), then Best weapon. |
| `strike_weapon` | melee.rs | Kill Aura and Trigger Bot hits swap here. Swap back is same tick or after Shortest–Longest delay ticks (`back_delay`). |
| `smash_intent` → `Smash::plan` | melee.rs, `mace_policy.rs` | Smash assist decides each tick: Idle, BreakShield (axe first), Hold or Smash. It holds while the target is still hittable next tick (`continuation` in `mace_native.rs`), while the target has hurt time, while you sprint, or until the charge is about to pass 0.9. One smash per fall. |
| `stun_slam` | melee.rs | Falling onto a raised shield: axe hit, then mace hit in the same tick. |
| Criticals "Mace smash" | runtime.rs, `crit_plan` | Meteor's packet spoof: a hop of 1.501 blocks + Additional height. |
| No Fall "Pause on mace" | runtime.rs, `NO_FALL_MACE` | Meteor's pause-on-mace. |

**Known gap:** `Smash.spent` is cleared only on the ground (`observe_grounded` and `plan`). After a Wind Burst
bounce, Smash assist therefore refuses the second smash, so Wind Burst chains don't work with Smash assist. Kill
Aura's own swap (`preferred_weapon`) doesn't check `spent`, so Kill Aura still swaps to the mace on the second fall.

## Sources

### Meteor Client

https://github.com/MeteorDevelopment/meteor-client, commit `e3a2e77` (2026-10-27), GPL-3.0.
Paths are under `src/main/java/meteordevelopment/meteorclient/`.

**`systems/modules/combat/AttributeSwap.java`, Smart mode.** This is Meteor's real weapon and mace swap.
- It listens to the attack events (`DoAttackEvent` / `AttackEntityEvent`). Before the attack packet goes, it swaps to
  the hotbar slot with the best score (`InvUtils.swap`).
- It swaps back after `swap-back-delay`, 2 ticks by default.
- It goes to an axe first if the target `isBlocking()`.
- It can prefer an unbreakable item to save durability: unbreakable scores 4, Unbreaking 0.05 per level.
- `only-on-weapon` limits it to chosen held weapon types.
- The scores are hand-tuned weights, not damage:

  | Item | Score |
  |---|---|
  | Any mace while falling (fall > 1.5, not gliding) | 40 |
  | Density | 50 + level × fallDistance × 2, falling only |
  | Wind Burst | level × 20, falling only |
  | Breach | level × target armour × 0.3, living targets with armour |

- The sword scores are separate: Fire Aspect 30 when the target isn't burning, Sharpness, Smite, Bane of Arthropods,
  Looting on mobs, Sweeping Edge, and Impaling on aquatic mobs.
- The current item only gets replaced by something with a strictly higher score.

**`systems/modules/combat/Criticals.java`, "smash-attack".** When the attack packet is sent with a mace in hand and
not gliding, it sends three position packets first: y+0, y+1.501+`additional-height`, y+0. All three have
onGround = false. The server then counts a fall of over 1.5 blocks. Sunset's Mace smash is the same idea.

**Other mace references:**
- `movement/NoFall.java`, `pause-on-mace`: No Fall does nothing while a mace is held.
- `combat/Hitboxes.java`, `mace`: the weapon filter includes the mace.
- `combat/KillAura.java`: the mace is in its weapon list.

**`utils/entity/DamageUtils.java`:** the mace bonus adds `0.5 * fallDistance` whenever Density is present, whatever
its level. That's a bug. Sunset's estimate uses the real enchantment effect, so it doesn't have this problem.

### Trouser-Streak MaceKill (Meteor addon)

https://github.com/etianl/Trouser-Streak, `src/main/java/pwn/noobs/trouserstreak/modules/MaceKill.java`. No licence
detected.

It runs on the outgoing attack packet with a mace in hand and a living target that is alive. It skips targets that
are blocking, invulnerable or in creative. It then cancels the attack and replaces it with:

1. N `Rot` packets (`# spam packets`, default 4) to raise the movement allowance.
2. A teleport to y + `Fall height` (default 22, up to 169), then back to where you were.
3. A swing, then the attack.

Other details:
- **Totem bypass (Paper):** up to 3 attacks, each from `Height Increase` (default 9) more blocks up. A hit during
  invulnerability frames only counts if it does more damage than the last one.
- **Use Offset:** lands back at a random small horizontal offset plus 0.01 up. It avoids lava, fire, magma, berry
  bushes, powder snow, entities and blocks, and caches the checks.
- It tells you to turn off Meteor Criticals' smash-attack so the two don't clash.

### LiquidBounce ModuleMaceKill

https://github.com/CCBlueX/LiquidBounce, `src/main/kotlin/net/ccbluex/liquidbounce/features/module/modules/combat/ModuleMaceKill.kt`,
GPL-3.0. Docs: https://github.com/CCBlueX/Documentation, `md/modules/combat/macekill.md`.

It's the same trick in about 40 lines.
- `FallHeight` defaults to 22 (range 1–170). It's lowered to the highest height where your hitbox wouldn't be in a
  block.
- Above 10 blocks it first sends ceil(h/10) empty move packets (the Paper path). Otherwise it sends 2.
- Then it warps up h blocks and warps back.

### Aoba MaceAura

https://github.com/Cocolots/Aoba-Client, `src/main/java/net/aoba/module/modules/combat/MaceAura.java`, GPL-3.0.

- It picks the nearest target in a radius, with filters for animals, monsters, players and friends.
- On a fully charged tick it sends ceil(h/10) status packets plus a move up `Height` (default 100).
- On the next tick it sends the same again, a move back down, and then attacks.
- The module marks itself as detected by NCP, Vulcan, AAC, Verus, Grim, Matrix and others.

### BlackOut-CE AutoMace (legit)

https://github.com/LimonTH/BlackOut-CE, `.../module/modules/legit/AutoMace.java`, GPL-3.0.

- It tracks the fall from its own height change each tick. Any upward move resets the count.
- At `Fall Threshold` (default 3) it switches to the first hotbar mace.
- On landing it switches back if `Restore Slot` is on.
- It's simple. Its fall tracking breaks on a Wind Burst bounce or a small bump.

### Nora-Tweaks MaceCombo

https://github.com/noramibu/Nora-Tweaks, `src/main/java/me/noramibu/tweaks/modules/MaceCombo.java`. **CC0**, so it can
be reused freely.

The combo loop:
- **Start:** a manual mace attack with a fall of at least 1.5 locks the target.
- **Auto attack:** in the air, falling ≥ 1.5, after launching (velocity.y > 0.5 or fall > 3), within `attack-range`
  (default 4.5). It waits for full charge (`getAttackStrengthScale(0) >= 1`) unless within `close-attack-range`
  (default 2.5). It can rotate to the target. There's a 20-tick cooldown between hits.
- **Relaunch on landing:**
  - It throws a wind charge from the offhand first, otherwise from a hotbar slot.
  - The pitch is 85° (`optimize-pitch`), otherwise 90°.
  - 2 ticks later it sets your upward velocity to 0.6, which is a client-side velocity write.
  - 5 ticks later it goes back to the mace slot.
- It also throws a wind charge while on the ground if the target is between 2 and 4.5 blocks away.
- **The combo ends** when the target dies, goes beyond 12 blocks horizontally, or after `max-combo-time` (30 s).
- **Lock first mace:** it keeps using the mace slot the combo started with.

### PVPEnhanceAddon (no licence; low quality)

https://github.com/Qurvo/PVPEnhanceAddon, `pvp-enhance-addon/src/main/java/com/pvpenhance/addon/modules/mace/`.

Ideas only. The code looks machine-written.
- **ForceMaceModule:** switches to the mace once the fall passes a threshold (2.5), and switches back on landing.
  Its "shield slam" hits with the axe, then the mace in the same tick, using hardcoded slots. It has an auto attack
  with a 20-tick cooldown.
- **TacticalWindCharge:** swaps between elytra and chestplate depending on the situation. It prefers the mace with a
  height advantage of at least 4.
- **WindBurstChainOptimizer:** predicts damage and draws bounce paths. Its formula thresholds are wrong (1.5/4.5/9.5
  instead of 3/8 blocks fallen).
- **WindBurstManagement:** counts bounces by guessing from your velocity.
- **PearlCatchNoMace:** throws a pearl ahead of the target, leading it by 8 ticks.

## Comparison

| | Sunset | Best elsewhere |
|---|---|---|
| Which mace | Real damage estimate | Meteor: hand-tuned scores |
| Breach | ≥ 15 % more by estimate | Meteor: level × armour × 0.3 |
| When it swaps | At the hit; back same tick or after a delay | Meteor: at the attack, back after 2 ticks. BlackOut/ForceMace: while falling, back on landing |
| Shield | Axe first; Stun slam | ForceMace: the same with fixed slots |
| Smash timing | Smash assist (continuation check, hurt time, charge) | Nora: full charge unless close |
| Fake fall | Criticals Mace smash (1.501 + extra) | MaceKill: 22–170 blocks, totem bypass |
| Combo / relaunch | None | Nora: wind charge on landing |

## Recommendations for the new Auto Weapon (Mace)

The first-pass list from before the Grim research. The plan in "Plan for the new Auto Weapon (Mace)" below replaces
it; this list is kept for its reasoning.

1. **Wind Burst chains.** Clear `Smash.spent` once you're moving up after a smash (velocity.y > 0, no ground) and the
   fall passes Min fall again. **Check on the rig first** how Horizon's `fall_distance` behaves after a bounce.
   Iustitia's notes (see the Grim section) say a real smash resets the fall, as vanilla does.
2. **Relaunch.** This is Nora's idea, done Sunset's way. On landing near the target:
   - Throw a wind charge, offhand first, else from a hotbar slot.
   - Get the downward look through the look queue (`sunset::turn`) and click with the game's real use.
   - Swap back in the same tick.
   - Don't write the velocity on the client.
   - Possible options: Off / On landing / When target below.
3. **Swap timing.** "At hit" (what Sunset does now) or "While falling": hold the mace from Min fall, go back on
   landing. In the second mode, a small upward bump shouldn't reset the count.
4. **Mace pick: Auto.** Wind Burst when another fall onto the target is likely, otherwise most damage. Density is
   already in `estimate_weapon`.
5. **Predicted smash damage** on the target, for example in Target ESP, using `estimate_weapon`.
6. Later: **elytra → chestplate** at the top of a glide before the smash, from PVPEnhance's TacticalWindCharge.

**Keep out of Auto Weapon:** the MaceKill teleport and totem bypass. They're blatant packet exploits that the
anticheats detect. If they're ever wanted, they belong next to Criticals' Mace smash, and no bypass should be claimed.

## Tests

The rig tests live in `tools/verify.py`. The existing ones to extend or put new tests next to are
`@test("Auto Weapon (Mace)")`, `"Auto Weapon (Stun slam)"`, `"Auto Weapon (Axe on shields)"`, `"Criticals"` and
`"Auto Weapon (settings from before 0.9)"`. That last one is the place for a config migration test if settings get
renamed. The rig runs on Linux only; the user runs it and tests in game by hand.

## Mace techniques against GrimAC

**Sources:**
- Grim: https://github.com/GrimAnticheat/Grim, commit `41f6f6e` (2026-11-02), GPL-3.0. It's a reference for what
  gets flagged; don't copy its code. Paths below are under `common/src/main/java/ac/grim/grimac/`.
- Clients: the public clients found by a GitHub code search for "mace" + "grim", plus a web search.

All of it was read statically.

**Summary:** no public mace exploit has real evidence of passing Grim. The only mace module with test evidence is
Prism's, and it's not an exploit: it automates real mechanics and keeps vanilla packet order.

### Grim has no mace-specific check

The only Grim file that mentions the mace is `utils/nmsutil/BlockBreakSpeed.java`. Mace techniques run into Grim's
general checks instead.

**Prediction** (`checks/impl/prediction/OffsetHandler.java`, `Simulation` in `config/en.yml`):
- An offset of `threshold` 0.001 blocks from the predicted movement is a violation.
- An offset of `immediate-setback-threshold` 0.1 sets you back immediately.
- While a setback is pending, `checks/impl/movement/SetbackBlocker.java` cancels your later move packets.

**Packet checks** (under `checks/impl/`):

| Check | What it flags | Experimental? |
|---|---|---|
| `packetorder/PacketOrderE` | A hotbar slot change (`HELD_ITEM_CHANGE`) while this tick already has an attack/stab, right click, inventory open, release, sneak/sprint change, bed leave, glide start or mount jump. The state comes from `PacketOrderProcessor` and resets on the tick packet (`CLIENT_TICK_END` on 1.21.2+). | Yes |
| `misc/Post` | A slot change, attack, interact, use item, place, dig, punch or abilities packet after the tick's move packet but before the next transaction | **No** |
| `packetorder/PacketOrderO` | Any non-async packet after the move packet but before `CLIENT_TICK_END` | Yes |
| `packetorder/PacketOrderI` | Combat, use, release or dig packets in an invalid tick order | Yes |
| `packetorder/PacketOrderJ` | A use item after an attack without the expected interaction packet | Yes |
| `badpackets/BadPacketsA` | The same slot sent twice in a row (the duplicate is cancelled) | No |
| `badpackets/BadPacketsE` | More than 19 move packets in a row that don't update position | No |
| `badpackets/BadPacketsJ` | The look in a use-item packet doesn't match the tick's look | No |
| `combat/Reach`, `combat/Hitboxes` | Hits out of reach or off the target | No |

**Experimental checks** are off unless the server sets `experimental-checks: true` (it defaults to false; see
`checks/Check.java`, which skips flags for experimental checks).

### Grim's issue tracker

- **#2444** (https://github.com/GrimAnticheat/Grim/issues/2444): attribute swapping (`attack_range`) caused false
  Reach flags. The developers treat a single attribute swap as part of modern PvP. Fixed in `b281a2d`.
- **#2601** (https://github.com/GrimAnticheat/Grim/issues/2601): two slot changes in one tick (sword → spear → mace,
  then the attack) cause a false Reach flag. It was closed as a duplicate of **#2517**, which is still open. A single
  swap doesn't flag.
- **Paper config:** the same issues note that Paper's `unsupported-settings.update-equipment-on-player-actions`
  defaults to true, which disables attribute swapping. As we understand vanilla `Player.attack`, the smash bonus is
  added after the charge scaling, so a mace swap for a smash should still pay off on such servers. A Breach swap
  mostly won't.
- **Vanilla client:** a Grim developer notes that a hotbar key changes the item on the next tick, while scrolling
  changes it immediately.

### Prism: the one mace module with Grim test evidence

https://github.com/AdamPaprotka/prism, MIT. Files under `src/main/java/dev/prismglass/`.

**Test harness** (`dev/GrimTest.java`, `dev/GrimCases.java`): it runs scripted cases against a local Grim server.
Each case logs START and END, and an orchestrator counts Grim's console alerts and setbacks between the two. The cases
cover legit play, KillAura, Criticals, BreachSwap, AutoMace and many movement modules.

The changelog says "Combat fixes (tested on Grim 2.3.74: 0 alerts)". That's the project's own claim; we haven't
checked it.

**`module/combat/AutoMace.java`:**
- **Fall:** worked out from the top of the arc since you last went up, because the client never gets the server's
  value.
- **WindJump:**
  - Triggers when on the ground, an enemy is within Range (4) and you have a mace and a hotbar wind charge.
  - It requests a server-side look of pitch 90.
  - Once the server pitch is ≥ 85, in one tick it selects the charge (`WeaponSwap.forThisHit`), uses it and requests
    a jump.
  - It gives up after 8 ticks; the cooldown is 20 ticks.
- **AutoHit:** once you've fallen MinFall (2.5), it aims at the target's centre and attacks.
- **Mace swap:** any hit while falling (yours or KillAura's) swaps to the best mace for that hit, scored as
  1 + Density × 2 + Breach.

**`manager/MovementHooks.java`:** for 40 ticks after the launch it steers toward the target's feet by picking a
W/A/S/D combination. Anything you press yourself wins, except plain W. It never writes velocity.

**`module/combat/BreachSwap.java`:** a hit's strength is ticks since your last attack ÷ the *held* item's charge
time. A mace charges in about 33 ticks against a sword's 12.5, so swapping at sword pace gave weak mace hits (7 damage
against the sword's 19). It only swaps once the mace itself would be charged to at least MinCharge (0.9). It skips
the swap when AutoMace can smash, and with OnlyArmored it skips targets without armour.

### Fake-fall modules (MaceKill) against Grim

These are Trouser-Streak, LiquidBounce, Vortex (`MaceKillModule.java`), Aoba MaceAura, Meteor Criticals'
smash-attack and Sunset's own Criticals "Mace smash".

- **Prediction:** a 1.5–22 block teleport is far over the 0.1 immediate-setback threshold.
- **Setback blocking:** while the setback is pending, SetbackBlocker cancels the following move packets. So the
  "back down" packet probably never arrives, and the fall may not count at all. This is inferred from the code, not
  tested.
- **Packet spam:** Trouser-Streak's rotation-only packets count toward BadPacketsE.
- **Admitted detection:** Aoba marks its own MaceAura as detectable by Grim, Vulcan, NCP, Matrix and others.
- **Easy signature:** Iustitia (https://github.com/ThoriaDevelopment/Iustitia, MIT, a client-side anticheat), check
  `MaceSmashCheck` in `scripts/checks.json`: a height jump over 1.5 within ±2 ticks of a mace attack, without two
  ticks of real descent (≤ -0.5 each) before it.
- **Iustitia's fall tracking** also confirms that a real smash resets the fall, as vanilla does, and that a wind
  charge launch restarts the count from its own height.

### Other claims found

None of these claims came with proof:
- **Truppware** (https://github.com/TruppyBoo/Truppware-Client, CC0): its README says it "bypasses all anticheats
  blatantly". Its `AutoMace.java` is still useful: it waits for a tick with no attack or use before swapping back,
  explicitly because of PacketOrderE.
- **Wurst fork** (https://github.com/abacusus/myWurst, GPL-3.0), `MaceHack.java`: claims a delay of 0 is "fine for
  ... Grim".
- **Silk** (https://github.com/hitomdev/Silkcc, MIT), `AutoMace.java`: switches mace by a Density threshold and does
  a stun slam. It mentions Grim but has no test evidence.
- **Omix** (https://github.com/trangphatmac-lgtm/Omix, MIT), `AutoWeapon.java`: locks the slot while its Grim NoSlow
  mode is active.
- **Web search:** forks and promotional pages that claim to "bypass Grim" had no test results.

### What Sunset does today, measured against Grim

1. **Kill Aura / Trigger Bot path: clean.** `queue_strike` / `strike_weapon_back` in runtime.rs put the weapon in hand
   a tick before the hit and back the tick after it ("Same tick" means no extra delay). That avoids PacketOrderE and
   Post.
2. **Stun slam would trip PacketOrderE** when experimental checks are on. It sends axe hit → slot change → mace hit in
   one tick (runtime.rs, around the comment "Stun slam: the mace follows the axe in the same tick").
3. **Spear reach matches #2601.** `spear_pass` in melee.rs sends spear → weapon → hit in one tick.
4. **Criticals "Mace smash" is a fake fall.** Expect Simulation flags and setbacks on Grim.
5. **Breach swap ignores charge.** `preferred_weapon` compares `estimate_weapon` values, which leave out charge.
   `smash_intent` reads `slot_strength(game, game.selected, ..)`, the held slot's charge, even when the mace will be
   swapped in.
6. **Wind Burst chains:** `Smash.spent` clears only on the ground (see "Known gap" above).

## Plan for the new Auto Weapon (Mace)

The user agreed to this plan on 2026-11-03. It aims to be **clean on Grim by construction**: never send anything a
vanilla client couldn't send, then prove it with zero alerts on our own Grim server.

**Wording rule:** don't claim the result "bypasses Grim". Once tested, write something like "no Grim flags in our
tests on Grim 2.3.x".

### Step 0: Horizon itself must be clean

Sunset runs on Horizon, not the vanilla client. Grim predicts vanilla movement and vanilla packet order, so any
difference in Horizon's physics or tick order gets flagged whatever the modules do.

Before any mace test:
- Play with no modules on Grim. It needs zero alerts.
- Confirm Horizon sends `ClientTickEnd` (1.21.2+).
- Confirm Horizon orders its packets as vanilla 1.21.11 does: slot → attack or use → swing → move packet → end of
  tick.

### Rules the module must never break

1. **Movement only through keys.** No position packets of our own and no velocity writes (prediction threshold
   0.001, immediate setback at 0.1). Nora's `setDeltaMovement(0.6)` jump and every MaceKill variant are out.
2. **Vanilla packet order each tick:** at most one slot change → attack or use → swing → move packet → end of tick.
   - Never change slot after an attack or use in the same tick (PacketOrderE).
   - Never send anything after the move packet (Post, PacketOrderO).
   - Never send the slot already held (BadPacketsA).
   - Never swap twice in one tick (Reach false flag, #2601 / #2517).
3. **One action per tick:** an attack or a use, never both (PacketOrderJ and the MultiActions checks).
4. **Use the look the server already has.** Aim through the look queue (`sunset::turn`). Throw a wind charge only
   once the tick's look in the use-item packet matches the move packet's (BadPacketsJ).
5. **Real reach.** Hit only when the target is in range along the look the server has (`continuation` in
   `mace_native.rs` already checks this). Don't count on Hitboxes.
6. **Real charge.** Judge a mace hit by the *mace's* charge, not the held item's.

### Features

| Feature | Basis | Change in Sunset |
|---|---|---|
| **Smash swap** | Already in Sunset (Kill Aura path) | Keep: the mace goes in hand a tick before the hit and back the tick after. |
| **Chain** | Iustitia (a real smash resets the fall), Nora | Re-arm `Smash.spent` after a Wind Burst bounce once you've fallen past Min fall again, instead of only on the ground. |
| **Stun slam over two ticks** | Grim PacketOrderE, Truppware | Axe hit in tick N, then mace slot + hit in tick N+1. The axe disables the shield for seconds, and the smash bonus doesn't depend on charge, so nothing is lost. The slot goes back the tick after that. |
| **Breach swap + Min charge** | Prism BreachSwap | Swap only if the mace would hit with at least Min charge (0.9), computed with the mace's attack speed. |
| **Smash assist charge** | Prism, Nora | `charge` / `charge_next` in `smash_intent` should use the mace slot when the mace will be swapped in. |
| **Relaunch** | Prism WindJump, Nora MaceCombo | On the ground with the target in range: turn the look to straight down through the queue. Once the server has pitch ≥ 85, throw the wind charge and press jump in the same tick. Offhand charge preferred (no swap at all). A hotbar charge is selected that tick and its slot goes back the *next* tick (PacketOrderE flags a slot change during a use). Options: Off / Offhand / Hotbar, plus a range. |
| **Air steer** | Prism MovementHooks | After a relaunch, press W/A/S/D toward the target's feet for up to about 40 ticks. Any key you press yourself, except plain W, wins. |
| **Swap timing** | BlackOut AutoMace, PVPEnhance ForceMace | At hit (current) / While falling: hold the mace from Min fall and go back on landing. Fall tracking must survive a small upward bump. |
| **Mace pick: Auto** | Meteor AttributeSwap (ideas only) | Wind Burst when another fall onto the target is likely, otherwise the most damage. Keep `estimate_weapon`. |
| **Spear reach fix** | Grim #2601 / #2517 | `spear_pass` must not send spear → weapon → hit in one tick. |
| **Criticals Mace smash** | MaceKill analysis | Label it blatant in the menu and keep it out of the mace module. |

**Order of work:**
1. The fixes to existing code: Chain, Stun slam over two ticks, Breach min charge plus Smash assist charge, and
   Spear reach.
2. Then Relaunch and Air steer.
3. Then the Grim rig profile.

### What to base it on

**Primary:**
- **Prism** (MIT): `AutoMace.java`, `BreachSwap.java`, `MovementHooks.java`, and `GrimTest.java` /
  `GrimCases.java` for the harness design.
- **Grim source** (GPL, reference only): `PacketOrderE`, `Post`, `PacketOrderO`, `BadPacketsA`, `BadPacketsJ`,
  `OffsetHandler`, `Reach`, `Hitboxes`. Plus issues #2444, #2517 and #2601.

**Secondary:**
- **Truppware `AutoMace.java`** (CC0): swaps back only in a quiet tick.
- **Nora-Tweaks `MaceCombo.java`** (CC0): the combo flow. Not its velocity write.
- **Meteor `AttributeSwap.java`** (GPL, ideas only): scoring and the held-weapon filter.
- **Iustitia `MaceSmashCheck` and its NoFall notes** (MIT): what detectors look for, and how the fall resets.

**Docs and vanilla behaviour:**
- **Vanilla source:** `MaceItem`, `Player.attack` (charge scaling, then the smash bonus) and the client tick
  (`handleKeybinds` → attack → move packet → tick end).
- **Numbers:** Horizon's own combat rules (`smash_bonus` in `melee_policy.rs`, the enchantment value effects) and the
  Minecraft Wiki for Mace, Density, Breach and Wind Burst.
- **Paper global config:** `unsupported-settings.update-equipment-on-player-actions`.

### Proving it: a Grim profile on the rig

Add a Grim profile to the Paper rig in `tools/verify.py`:
- **Server:** Paper 1.21.11 with the latest Grim 2.3.x and `experimental-checks: true`. Test with
  `update-equipment-on-player-actions` set to both true and false.
- **Cases**, each with START/END markers, Prism-style:
  - legit play with no modules (Step 0)
  - manual smash
  - Smash assist
  - Kill Aura smash swap
  - Stun slam
  - Breach swap
  - Relaunch
  - Chain
  - Spear reach
- **Count** Grim's alerts (from the console or `/grim` verbose) and server setbacks between START and END for each
  case.
- **Pass bar:** 0 alerts and 0 setbacks over repeated runs. Record the Grim version with the result.

## Implementation status (branch `auto-weapon-mace`, 2026-11-03)

Built (native build `native-ba6344a-fb8ecec88c71`); not yet tested on a Grim server.

**Finding while implementing:** Horizon sends the tick's move packet inside the tick (`tick_remote_level`), and
Sunset's `post_tick` runs after it, before `ClientTickEnd`. Any slot change, hit or use sent from `post_tick` is
flagged by Grim's **Post** (not experimental) and **PacketOrderO**. The safe places are Sunset's `pre_tick` (before
the tick's input and move packet) and, for inventory clicks, the frame hook (between ticks).

Done:
- **Weapon swaps at the start of the tick.** `queue_strike` only records the slot; `send_strikes` selects it just
  before the hit. `strike_weapon_back` and the melee lease's return (`lease_back`) run at the start of the next tick,
  before any hit or use, never two slot changes in a tick.
- **Stun slam over two ticks.** Axe hit, then `send_slam` (mace selected, hit) at the start of the next tick.
- **Chain.** `Smash` in `mace_policy.rs` now tracks the server's fall after a smash (reset at the smash, counted from
  the smash height or the top of the rise after it). `spent` is gone; every fall check goes through `smash_fall`.
- **Breach swap Min charge** (0.9) by the mace's own attack speed; **Smash assist charge** reads the mace's slot.
- **Smash assist hold** stops sprinting by the key in `pre_input` (the game sends the command before the move
  packet), not by a PlayerCommand after it.
- **Relaunch** (Off / Offhand / Hotbar, Relaunch range): look to pitch 90 through the queue, then at the next tick's
  start the UseItem with that exact look, the look kept for that tick's move packet (BadPacketsJ), jump key held, a
  hotbar charge's slot back the tick after. 20-tick cooldown; needs 3 blocks of room above.
- **Air steer:** W/A/S/D toward the target for up to 40 ticks after a Relaunch; your A/S/D win.
- **Spear reach** never adds its two slot changes after Auto Weapon's. Alone it still sends two (Grim #2601/#2517):
  keep it off on Grim.

Not done: Swap timing "While falling", Mace pick Auto, Criticals Mace smash label, the Grim rig profile. Other
modules still send from `post_tick` (for example Key Throw's slot + UseItem + slot back) and will trip Post.

### ACtest round 1 (2026-11-03): flags and fixes

| Flag | Cause | Fix |
|---|---|---|
| BadPacketsA (slot pairs) | `hold` sent SetCarriedItem without updating Horizon's `Remote.carried`, so the game's next sync sent the same slot again. | Upstream `6dc3dfc`: `hold` records the slot through `tell_carried` (our own `outgoing` drop was dropped in the rebase). |
| AimDuplicateLook | Auto Weapon's own hits queue your current look; the move packet then carried a rotation equal to the last one sent. | Upstream `6dc3dfc`: `LAST_LOOK` in `outgoing` and `end_turns`; `end_turns` compares the look `wire_look` will send. |
| AimModulo360 | The look queue folded the yaw into ±180, a 360° jump whenever your yaw had wound past it. | Raw looks are queued; `wire_look` sends the yaw nearest your own, then nearest `last_look()`. Relaunch's UseItem uses `wire_yaw`. |
| Simulation (decaying ×0.91 in air, ×0.546 on ground) | A sprint hit on a player at over 0.9 charge: Grim counts the attacker's 0.6 speed cut as certain (`PacketPlayerAttack` → `minAttackSlow`), but Horizon's remote attack and Sunset's hits never apply it. | `combat_wire` sees every Attack on the wire and queues the cut; the `screen_keys` hook applies ×0.6 and stops the sprint at the start of the next movement. |

Not done: no movement fix for looks that differ from your own (Kill Aura and the like still move with the camera's yaw).

### ACtest round 2 (2026-11-03): Simulation only against a moving target

Falling with Auto Weapon's mace swap, a moving target gave Simulation (offsets ~0.0035 decaying ×0.91 in the air,
×0.546 on the ground) and no hit; a still target was clean. The 0.6 attack cut's charge check used the attack speed of
the slot just selected (the mace, 33 ticks to charge), but the game, the server and Grim use the item the last tick held
(`attack_item`, as `detectEquipmentUpdates` gives modifiers in the player's tick): a long-charged sword swapped for a mace
in the hit's tick still hits charged, so Grim requires the cut and the client skipped it. Against a still target Smash
assist holds first, and holding unsprints, so the cut never came up. Fix: `attack_slow` uses
`hand_strength.scale(0.5, main_hand_attack_speed())`, the game's own charge.
