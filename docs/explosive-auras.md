# Crystal Aura and Anchor Aura

Experimental native Rust combat for Horizon `ba6344a`. Both modules start off.
This branch matches the inspected 1.0 behavior contract; [parity notes](combat-parity.md)
and [validation](combat-validation.md) distinguish implementation from live proof.
The client interface keeps Sunset branding.

## Controls

Crystal Aura exposes 16 controls: Require Hold, Hold Key, Place Delay, Break
Delay, Min Damage, Max Self Damage, Face Place, Force Face Place, Auto Obsidian,
Switch Back, Turn Speed, View Angle, Human Pace, Protect Loot, Render and Render
Color. Place/Break delays default to zero; damage defaults are 4 target and 8
self. Face Place, Auto Obsidian, Switch Back, Human Pace, Protect Loot and Render
start enabled. Require Hold starts disabled; its default key is mouse4.

Anchor Aura exposes six controls: Min Damage, Max Self Damage, Render, Render
Color, Turn Speed and Protect Loot. Damage defaults are 6 target and 6 self.
There are no entity selectors, configurable reach, separate Place/Break toggles,
selectable damage model or Obsidian Delay on these Auras.

Open Combat in the ClickGUI or use `.settings crystalaura` and `.settings
anchoraura`. Supply the relevant materials in the hotbar/offhand. Crystal can
borrow a hotbar slot while a combat item or empty hand is selected; arbitrary
held items intentionally yield. Anchor additionally yields while crouching,
with offhand glowstone, or when the native camera state prevents its turns.
It does not detonate anchors in a dimension where anchors are safe respawn
points. The manually controlled Anchor Macro has separate hand/mode rules.

## Planning and actions

Targets are current, alive enemy players with valid tab-list mode and native
team/friend checks. Focus, the Crystal bot's preferred target and weak opponents
influence the top-three selection. Known friends remain protected outside the
ten-block enemy search. Native block/entity interaction attributes determine
reach; a proposed crystal must also be within future attack reach.

Six staged action kinds distinguish immediate attacks, opening-window attacks,
crystal placements, obsidian setup and anchor setup. Ready useful attacks take
precedence over redundant setup. Apparent lethal damage considers effective
health and a visible held totem; it is a ranking estimate, not a death prediction.
Anchor attacks need a one-point advantage over an available Crystal opening.
A bounded probe keeps ineffective existing anchors from starving new placements.

Crystal placement waits for a real matching entity. Anchor place, charge and
detonate each wait for the corresponding actual block-state change. No guessed
entity ID, fabricated charge, local removal or simulated damage is accepted as
an authoritative receipt. Separate Crystal action clocks retain their deadlines
through temporary pauses. Human Pace adds a bounded two-to-four-tick gap unless
the Crystal bot explicitly requests full speed; zero delay still does not send
multiple accepted deeds in one tick.

Both Auras share one temporary hand baseline and one deferred action owner.
Fresh geometry, materials, settings, damage and ownership are checked again
before forwarding. Menu/manual input and competing work cancel unsent deeds;
actual pending acknowledgements and retry history survive ordinary pauses.
Slot return checks ownership and user edits, and defers behind an emergency
hand action. Login, respawn and disconnect discard pre-boundary deeds and hand
state, including a same-dimension respawn on the same connection.

## Damage and prediction

Exposure and armor/toughness/resistance/protection reduction use native kernels.
Current Auras use the ordinary observed-state estimate, not the historical
selectable Unmitigated/equipped-upper safety mode. Face Place separately reads
observed armor durability. Hidden attributes/effects, equipment changes or
breakage and stale difficulty snapshots remain limits; this is an estimate.

RTT comes from actual placement-to-matching-spawn receipts, with local tab ping
as fallback. Read-only projections use native contextual collisions, chunk
availability and the world border. They never move a real entity. Placement and
break use their respective bounded forecast horizons. Future obsidian changes
only the copied exposure geometry used to evaluate setup.

The hurt ledger tracks accepted damage observations. Repeated offensive damage
uses raw incremental damage during the hurt window before armor/magic reduction.
Self and friend protection still evaluate the complete blast. Noncreative
survival requires `damage * 1.15 + 2 < health + absorption`, as well as the self
cap; holding a totem does not waive this check. Friend safety uses observed
armor/equipment and the same complete-blast/survival checks. Unknown observations,
malformed values or exhausted work refuse the action.

Face Place activates for the configured key or enabled low effective health,
horizontal resistant cover, or low armor durability. It caps the placement
minimum at 2 and scales the break minimum by 0.6. Lethal bypass is evaluated
against the reduced incremental damage, not stale full-blast damage.

## Bounds and evidence

Crystal advances at most 96 scan cells and evaluates four candidate actions per
planning pass with 24 prediction and 4096-ray budgets. Anchor advances 128 cells,
evaluates four candidates and uses 48 prediction and 4096-ray budgets. Both
retain bounded server waits/retries. The shared movement predictor has its own
512-step budget and 32-track cache. These are separate bounds, not an FPS claim.

Auto Obsidian uses real support/native preview checks and a 64-cell ledger with
three sends per unchanged state and a 30-tick wait. Actual obsidian payload and
matching world state are required before fresh Crystal scoring. Existing terrain
is never mined or overwritten to create a base.

Protect Loot defaults on and pauses planned blasts near recorded player deaths
for 30 seconds. It protects locations rather than following item entities; see
[death-guard scope](combat-modules.md). Optional rendering keeps at most two
native world boxes and damage labels. No reference graphics/assets are bundled.

`.combatdebug` formats diagnostics only on request. Counts of planned/sent work
are not server confirmations. Historical core-batch screenshots and fixtures
remain labeled with their original build; they do not prove this branch's new
settings, prediction or staged actions. Current private-player evidence must
identify the exact frozen client/package and retain failed attempts.
