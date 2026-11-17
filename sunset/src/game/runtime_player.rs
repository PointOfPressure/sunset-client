// Runtime part (see runtime.rs): blocks and items, Scaffold, Legit Sneak, Auto Eat, block scans and inventory clicks.

use super::Game;
use super::sunset_util::block_pos;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use winit::keyboard::KeyCode;
use super::sunset::*;

pub(super) fn solid_at(game: &Game<'_>, x: i32, y: i32, z: i32) -> bool {
    !game.res.facts().is_air(game.world.block(x, y, z))
}

// The block column just ahead of the player's feet, in the direction the
// move keys point: its x and z, and the y of the block under the feet.
// Parkour (Sunset's), before the tick moves you: on the ground, moving, jump not held and not sneaking. Last tick:
// jump when your box where this tick's move takes you (as far as the last tick's), Edge distance narrower each side
// and half a block down, would touch no block, so you leave from the last spot on it. Early: when a point 0.35 blocks
// ahead of you is over air.
pub(super) fn parkour_jump(game: &Game<'_>, m: &Module) -> bool {
    use horizon_core::geometry::Vec3;
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let (forward, side) = move_input(game);
    let held = |k: KeyCode| game.keys.contains(&k);
    if !p.ground.on_ground
        || held(KeyCode::Space)
        || (forward == 0.0 && side == 0.0)
        || (held(KeyCode::ShiftLeft) && !m.b("Ignore sneaking"))
        || (m.b("Only while sprinting") && !p.sprinting)
        || (m.b("Only when moving forward") && forward <= 0.0)
    {
        return false;
    }
    if m.p("Jump when") == "Early" {
        return ahead(game).is_some_and(|(x, y, z)| !solid_at(game, x, y, z));
    }
    let step = Vec3::new(p.position.x - game.previous_position.x, -0.5, p.position.z - game.previous_position.z);
    // Each side's edge distance rolled between Edge distance and Longest edge distance (the same when they match).
    let mut rng = super::sunset_rotation::Rng::new(game.ticks.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x51ED);
    let (lo, hi) = (m.n("Edge distance"), m.n("Longest edge distance").max(m.n("Edge distance")));
    let mut edge = || lo + rng.next() * (hi - lo);
    let under = p.bounds().move_by(step).deflate(Vec3::new(edge(), 0.0, edge()));
    horizon_client::collisions::SceneCollisions { world: &game.world, binding: game.collision, subject: None }
        .unobstructed(under)
        .unwrap_or(false)
}

// Safe Walk's Sneak: a block item in either hand (with Only holding blocks), your look at least Min pitch down, and
// the block under you and the two below it all air or replaceable, so you are past the edge of what you stand on.
pub(super) fn edge_sneak(game: &Game<'_>, m: &Module) -> bool {
    let block = |s: Option<&horizon_client::items::ItemStack>| s.is_some_and(|s| game.res.items().get(&s.item).is_some_and(|i| i.block.is_some()));
    if (m.b("Only holding blocks") && !block(game.held()) && !block(game.offhand.as_ref())) || f64::from(game.pitch) < m.n("Min pitch") {
        return false;
    }
    super::sunset_util::block_pos(game).is_some_and(|[x, y, z]| (1..=3).all(|d| super::sunset_util::replaceable(game, [x, y - d, z])))
}

pub(super) fn ahead(game: &Game<'_>) -> Option<(i32, i32, i32)> {
    let p = game.player.as_ref()?;
    let d = move_direction(game)?;
    let (ax, az) = (p.position.x + d[0] * 0.35, p.position.z + d[1] * 0.35);
    Some((ax.floor() as i32, (p.position.y - 0.5).floor() as i32, az.floor() as i32))
}

// Switches the held slot now: the server is told before anything is sent
// that depends on it, once (the tick sees it was told and doesn't send it again).
pub(super) fn hold(game: &mut Game<'_>, slot: usize) {
    game.selected = slot;
    if super::remote::sunset_remote::tell_carried(game, slot) {
        game.send(S::SetCarriedItem(play::SetCarriedItem { slot: slot as i16 }));
    }
}

pub(super) fn item_info(game: &Game<'_>, slot: usize) -> Option<horizon_client::items::ItemInfo> {
    let stack = game.inventory.get(slot)?.as_ref()?;
    game.res.items().get(&stack.item).cloned()
}

pub(super) fn best_weapon(game: &Game<'_>, prefer_axes: bool) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for slot in 0..9 {
        let Some(stack) = game.inventory.get(slot).and_then(|s| s.as_ref()) else {
            continue;
        };
        let Some(info) = game.res.items().get(&stack.item) else {
            continue;
        };
        if info.attack_damage <= 0.0 {
            continue;
        }
        let kind = if stack.item.ends_with("_axe") && prefer_axes {
            100.0
        } else if stack.item.ends_with("_sword") && !prefer_axes {
            100.0
        } else {
            0.0
        };
        let score = info.attack_damage + kind;
        if best.is_none_or(|(_, b)| score > b) {
            best = Some((slot, score));
        }
    }
    best.map(|(slot, _)| slot)
}

// Scaffold's blocks: a block item in the list (or not in it, as a blacklist) that is a full block and does not fall.
pub(super) fn scaffold_item(game: &Game<'_>, m: &Module, slot: usize) -> bool {
    let Some(state) = item_info(game, slot).and_then(|i| i.block) else {
        return false;
    };
    let f = game.res.facts().get(state);
    let name = super::sunset_util::short(&f.name);
    let listed = super::sunset_util::listed(m.t("Blocks"), name);
    let falls = ["sand", "gravel", "concrete_powder", "anvil"].iter().any(|e| name.ends_with(e)) || name == "dragon_egg";
    listed == (m.p("Blocks filter") == "Whitelist") && f.full_collision && !falls
}

// Scaffold's presets: settings of its modes made for one style, filled in when the Preset is picked (not on the first
// look, so a change you made afterwards stays). Speedbridge is Legit's edge sneaking at its quickest; Blatant is Normal
// with snap rotations, no ray cast, sprint kept, a reach ahead and fast tower while moving. Neither adds an algorithm.
pub(super) const SCAFFOLD_PRESETS: [(&str, &[(&str, &str)]); 2] = [
    ("Speedbridge", &[("Mode", "Legit"), ("Shortest sneak delay (ms)", "0"), ("Longest sneak delay (ms)", "50")]),
    ("Blatant", &[("Mode", "Normal"), ("Rotate", "true"), ("Rotations", "Snap"), ("Look", "Silent"), ("Ray cast", "Off"),
        ("Movement correction", "Silent"), ("Sprint", "Keep"), ("Expand", "2"), ("Same Y", "Off"), ("Ledge sneak", "false"),
        ("Fast tower", "true"), ("While moving", "true")]),
];
static SCAFFOLD_PRESET: Mutex<String> = Mutex::new(String::new());

pub(super) fn scaffold_preset(game: &Game<'_>, m: &Module) {
    let preset = m.p("Preset");
    let before = std::mem::replace(&mut *SCAFFOLD_PRESET.lock().unwrap_or_else(|e| e.into_inner()), preset.to_owned());
    if before.is_empty() || before == preset {
        return;
    }
    if let Some((_, fields)) = SCAFFOLD_PRESETS.iter().find(|p| p.0 == preset) {
        for (name, value) in fields.iter() {
            set_setting(game, &format!("scaffold.{}", norm(name)), value);
        }
    }
}

// Scaffold waits while Blink is on, and while Kill Aura has someone to hit if Disable with Kill Aura says so (Kill
// Aura on with no one about leaves Scaffold be; without the setting Kill Aura waits for Scaffold instead).
pub(super) fn scaffold_paused(m: &Module, kill_aura_engaged: bool, blink: bool) -> bool {
    blink || (kill_aura_engaged && m.b("Disable with Kill Aura"))
}

// The biggest stack in the hotbar (the one in hand if two are level), kept until it runs out: taking the biggest
// again each time would swap between two like stacks at every block. Auto switch off: only what is in hand.
pub(super) fn scaffold_slot(game: &Game<'_>, m: &Module) -> Option<usize> {
    if !m.b("Auto switch") {
        return Some(game.selected).filter(|&i| scaffold_item(game, m, i));
    }
    if let Some(kept) = with(|s| s.scaffold_stack).filter(|&i| scaffold_item(game, m, i)) {
        return Some(kept);
    }
    let count = |i: usize| game.inventory.get(i).and_then(|s| s.as_ref()).map_or(0, |s| s.count);
    let best = (0..9).filter(|&i| scaffold_item(game, m, i)).max_by_key(|&i| (count(i), i == game.selected, std::cmp::Reverse(i)));
    with(|s| s.scaffold_stack = best);
    best
}

// The blocks Scaffold can use: the hotbar's, or only the hand's with Auto switch off.
pub(super) fn scaffold_blocks(game: &Game<'_>, m: &Module) -> u32 {
    let count = |i: usize| game.inventory.get(i).and_then(|s| s.as_ref()).map_or(0, |s| s.count);
    if !m.b("Auto switch") {
        return if scaffold_item(game, m, game.selected) { count(game.selected) } else { 0 };
    }
    (0..9).filter(|&i| scaffold_item(game, m, i)).map(count).sum()
}

// A block Scaffold placed, drawn for 8 ticks as it fades (Render).
pub(super) fn scaffold_drawn(at: [i32; 3]) {
    with(|s| s.scaffold_placed.push((at, 8)));
}

// Normal's Look Visible is about to turn your view: the view before it is kept, to be put back when Scaffold goes off.
pub(super) fn scaffold_view(game: &Game<'_>) {
    let view = (game.yaw, game.pitch);
    with(|s| {
        s.scaffold_rotation.get_or_insert(view);
    });
}

// Scaffold's placed blocks fade (its Normal mode works before the tick, in sunset_scaffold_normal).
pub(super) fn scaffold_fade() {
    with(|s| {
        s.scaffold_placed.retain_mut(|(_, t)| {
            *t = t.saturating_sub(1);
            *t > 0
        });
    });
}

// Legit Scaffold's sneak: held (the game's movement then keeps you on the edge, and tells the server you sneak) or let
// go, if it was Scaffold's and you are not holding it yourself.
pub(super) fn legit_sneak(game: &mut Game<'_>, down: bool) {
    let ours = with(|s| s.scaffold_sneak);
    if down && !game.keys.contains(&KeyCode::ShiftLeft) {
        game.keys.insert(KeyCode::ShiftLeft);
        with(|s| s.scaffold_sneak = true);
    } else if !down && ours {
        if !game.physical_keys.contains(&KeyCode::ShiftLeft) {
            game.keys.remove(&KeyCode::ShiftLeft);
        }
        with(|s| s.scaffold_sneak = false);
    }
}

// Auto Eat (Sunset's): once health or hunger is at its threshold, holds use on the best food (the off hand's first,
// then the hotbar's, then the inventory's moved into the hotbar), with Kill Aura off meanwhile, until neither is.
// Each tick it eats from the food it chose: held again if something else took the hand (your keys, Auto Tool,
// Scaffold), and use is only pressed with food in that hand, so it never uses or places what took its place. A
// menu, death or Auto Gap stops it, and it starts afresh after.
pub(super) fn auto_eat(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    let alive = game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    if !game.native_playing() || !alive || super::sunset_player::gap_eating() {
        if with(|s| s.eating) {
            eat_stop(game);
        }
        return;
    }
    if with(|s| s.eating) {
        if !super::sunset_combat_control::claim("Auto Eat") || !eat_due(game, m) {
            eat_stop(game);
            return;
        }
        let slot = with(|s| s.eat_slot);
        let edible = match slot {
            Some(i) => edible(game, m, game.inventory.get(i).and_then(|s| s.as_ref())),
            None => edible(game, m, game.offhand.as_ref()),
        };
        if !edible && !food_slot(game, m).is_some_and(|next| eat_from(game, next)) {
            eat_stop(game);
            return;
        }
        // The hand went to something else: let go, take the food back (or, eating from the off hand, an empty
        // hand), and press again next tick.
        let want = match with(|s| s.eat_slot) {
            Some(i) => Some(i),
            None => (!hand_inert(game)).then(|| empty_hotbar(game)).flatten(),
        };
        if let Some(i) = want
            && game.selected != i
        {
            let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
            hold(game, i);
            return;
        }
        if with(|s| s.eat_slot).is_none() && !hand_inert(game) {
            eat_stop(game);
            return;
        }
        if game.using.is_none() {
            let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, true);
        }
        return;
    }
    if !eat_due(game, m) {
        return;
    }
    let Some(slot) = food_slot(game, m) else {
        return;
    };
    let prev = game.selected;
    if !super::sunset_combat_control::claim("Auto Eat") {
        return;
    }
    if !eat_from(game, slot) {
        super::sunset_combat_control::release("Auto Eat");
        return;
    }
    let paused = m.b("Pause auras") && with(|s| s.is_on("Kill Aura"));
    if paused {
        toggle_named(game, "Kill Aura", false);
    }
    with(|s| {
        s.eating = true;
        s.eat_prev = prev;
        s.eat_paused_aura = paused;
    });
    let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, true);
}

// Health and hunger against the thresholds, as Threshold mode joins them.
fn eat_due(game: &Game<'_>, m: &Module) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let health = f64::from(p.health.health()) <= m.n("Health threshold");
    let bar = f64::from(p.health.food.food) <= m.n("Hunger threshold");
    let saturation = f64::from(p.health.food.saturation) <= m.n("Saturation threshold");
    let hunger = match m.p("Food mode") {
        "Saturation" => saturation,
        "Either" => bar || saturation,
        _ => bar,
    };
    match m.p("Threshold mode") {
        "Health" => health,
        "Hunger" => hunger,
        "Both" => health && hunger,
        _ => health || hunger,
    }
}

// Food that passes the list (off the Blacklist, or on the Whitelist) and that you can eat now (hungry, or food you can always eat).
fn edible(game: &Game<'_>, m: &Module, stack: Option<&horizon_client::items::ItemStack>) -> bool {
    let Some(stack) = stack else {
        return false;
    };
    let Some(info) = game.res.items().get(&stack.item) else {
        return false;
    };
    let hungry = game.player.as_ref().is_some_and(|p| p.health.food.food < 20);
    info.food.is_some() && super::sunset_util::passes(m, if m.p("List mode") == "Whitelist" { "Whitelist" } else { "Blacklist" }, &stack.item) && (hungry || info.can_always_eat)
}

// A right click with this in the main hand does nothing, so the off hand eats: an empty hand, a sword, pickaxe or
// mace. (A block would be placed, and axes, hoes, shovels, spears, pearls and the like have uses of their own.)
fn hand_inert(game: &Game<'_>) -> bool {
    game.held().is_none_or(|s| ["_sword", "_pickaxe", ":mace"].iter().any(|t| s.item.ends_with(t)))
}

fn empty_hotbar(game: &Game<'_>) -> Option<usize> {
    (0..9).find(|&h| game.inventory.get(h).is_some_and(Option::is_none))
}

// Where to eat from: None for the off hand (while the main hand does nothing, or an empty hotbar slot can take it),
// else the inventory slot (0 to 8 the hotbar) of the best food.
fn food_slot(game: &Game<'_>, m: &Module) -> Option<Option<usize>> {
    if edible(game, m, game.offhand.as_ref()) && (hand_inert(game) || empty_hotbar(game).is_some()) {
        return Some(None);
    }
    let value = |i: usize| -> Option<f32> {
        let stack = game.inventory.get(i)?.as_ref();
        let (nutrition, saturation) = game.res.items().get(&stack?.item)?.food?;
        edible(game, m, stack).then(|| match m.p("Food priority") {
            "Combined" => nutrition as f32 + saturation,
            "Hunger" => nutrition as f32,
            _ => saturation,
        })
    };
    let best = |range: std::ops::Range<usize>| {
        range.filter_map(|i| value(i).map(|v| (i, v))).fold(None, |b: Option<(usize, f32)>, (i, v)| if b.is_none_or(|b| v > b.1) { Some((i, v)) } else { b })
    };
    let found = best(0..9).or_else(|| if m.b("Search inventory") { best(9..36) } else { None });
    found.map(|(i, _)| Some(i))
}

// Holds the food: the off hand (with an empty main hand if what it holds would act first), a hotbar slot selected,
// or food from the inventory moved to an empty hotbar slot first. False when there is no room for it.
fn eat_from(game: &mut Game<'_>, slot: Option<usize>) -> bool {
    let at = match slot {
        None => {
            if !hand_inert(game) {
                let Some(empty) = empty_hotbar(game) else {
                    return false;
                };
                hold(game, empty);
            }
            with(|s| s.eat_slot = None);
            return true;
        }
        Some(i) if i < 9 => i,
        Some(i) => {
            let Some(empty) = empty_hotbar(game) else {
                return false;
            };
            if !menu_click(game, &[(i, horizon_client::menu::Input::Pickup), (empty, horizon_client::menu::Input::Pickup)]) {
                return false;
            }
            empty
        }
    };
    hold(game, at);
    with(|s| s.eat_slot = Some(at));
    true
}

pub(super) fn eat_stop(game: &mut Game<'_>) {
    super::sunset_combat_control::release("Auto Eat");
    use winit::event::MouseButton;
    let (prev, aura) = with(|s| {
        s.eating = false;
        (s.eat_prev, std::mem::take(&mut s.eat_paused_aura))
    });
    let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
    hold(game, prev);
    if aura {
        toggle_named(game, "Kill Aura", true);
    }
}

// Debug Report: each module on but Debug Report, with its bind and the settings that differ from their defaults
// (name, value, default; a plugin's module lists all of them), and the profile in use.
pub(super) type ModuleState = (&'static str, Option<String>, Vec<(&'static str, String, String)>);

pub(super) fn debug_modules() -> (String, Vec<ModuleState>) {
    let defaults = defaults();
    with(|s| {
        let list = s
            .modules
            .iter()
            .filter(|m| m.on && m.name != super::sunset_debug::NAME)
            .map(|m| {
                let base = defaults.iter().find(|d| d.name == m.name);
                let changed = m
                    .settings
                    .iter()
                    .filter_map(|x| {
                        let default = base.and_then(|d| d.setting(x.name));
                        default
                            .is_none_or(|d| d.value != x.value)
                            .then(|| (x.name, x.value_text(), default.map_or_else(|| "-".to_owned(), Setting::value_text)))
                    })
                    .collect();
                (m.name, m.key.clone(), changed)
            })
            .collect();
        (s.selection.label(), list)
    })
}

pub(super) fn toggle_named(game: &mut Game<'_>, name: &str, on: bool) {
    if let Some(i) = with(|s| s.find(name)) {
        toggle(game, i, on);
    }
}

pub(super) fn block_colour(preset: &str, name: &str) -> Option<u32> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    let ore = |n: &str| -> Option<u32> {
        let core = n.strip_prefix("deepslate_").unwrap_or(n);
        Some(match core {
            "diamond_ore" => 0xFF55_FFFF,
            "emerald_ore" => 0xFF55_FF55,
            "gold_ore" | "nether_gold_ore" => 0xFFFF_D040,
            "iron_ore" => 0xFFD8_AF93,
            "coal_ore" => 0xFF90_9090,
            "redstone_ore" => 0xFFFF_3030,
            "lapis_ore" => 0xFF30_50FF,
            "copper_ore" => 0xFFE0_7040,
            "nether_quartz_ore" => 0xFFEE_EEEE,
            _ => return None,
        })
    };
    match preset {
        "Ores" => ore(n).or((n == "ancient_debris").then_some(0xFFB0_6040)),
        "Diamond" => matches!(n, "diamond_ore" | "deepslate_diamond_ore").then_some(0xFF55_FFFF),
        "Ancient Debris" => (n == "ancient_debris").then_some(0xFFB0_6040),
        "Spawners" => matches!(n, "spawner" | "trial_spawner").then_some(0xFF80_40FF),
        "Portals" => matches!(n, "nether_portal" | "end_portal" | "end_portal_frame" | "end_gateway").then_some(0xFFB0_50FF),
        _ => None,
    }
}

// Block ESP's search of the loaded blocks around the player, spread over ticks: again when the player has moved, the
// settings changed, or it has been a while.
pub(super) fn scan_blocks(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(at) = block_pos(game) else {
        return;
    };
    let preset = m.p("Blocks");
    let range = m.n("Range") as i32;
    let kind = ["Ores", "Diamond", "Ancient Debris", "Spawners", "Portals"].iter().position(|p| *p == preset).unwrap_or(0);
    let key = (kind as u64) << 32 | range as u64;
    let facts = game.res.facts();
    with(|s| {
        s.blocks.tick(ticks, at, range, key, 100, 6, |c, x, out| {
            let ys = (c[1] - range).max(-64)..=(c[1] + range).min(320);
            for z in c[2] - range..=c[2] + range {
                for y in ys.clone() {
                    let state = game.world.block(x, y, z);
                    if facts.is_air(state) {
                        continue;
                    }
                    if let Some(col) = block_colour(preset, &facts.get(state).name) {
                        out.push(([x, y, z], col));
                    }
                }
            }
            (2 * range as usize + 1) * ys.count()
        })
    });
}

// A click or several in the player's own inventory (the menu with no
// container open), by the menu's slot numbers: 0 to 8 the hotbar, 9 to 35
// the rest, 500 to 503 the armor.
// Clicks on inventory slots; none at all when one is a slot Loadouts locks.
// Clicks in the player's inventory, for modules that click on their own: only while the server knows you stand still
// (no movement keys, jump, sneak or sprint in the last input sent), as with any screen open: a click while moving is
// what Grim's MultiActionsC flags (and cancels). False when nothing was clicked; the caller tries again later.
pub(super) fn menu_click(game: &mut Game<'_>, ops: &[(usize, horizon_client::menu::Input)]) -> bool {
    if super::sunset_gui_move::inventory_busy() { return false; }
    if ops.iter().any(|&(slot, _)| super::sunset_loadouts::locked(slot)) || !super::remote::sunset_combat_remote::quiet(game) {
        return false;
    }
    let ops: Vec<(Option<usize>, i32, horizon_client::menu::Input)> =
        ops.iter().map(|&(slot, input)| (Some(slot), 0, input)).collect();
    game.remote_menu_ops(&ops)
}

pub(super) fn inventory_idle(game: &Game<'_>) -> bool {
    !super::sunset_gui_move::inventory_busy() && game.native_playing() && game.remote.is_some() && game.remote_menu().1 == "inventory"
}
