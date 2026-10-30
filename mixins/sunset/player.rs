// More of Meteor's player and misc modules: Auto Fish, Auto Gap, Fast Use,
// Spam, and Anchor.

use super::sunset::{Category, Module, hold, look_at};
use super::sunset_util::*;
use super::{Game, Screen};
use std::sync::Mutex;
use horizon_protocol::packets::{Empty, play::{self, Serverbound as S}};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        Module::new("Auto Fish", Player, "Casts and reels in for you.")
            .bool("Auto switch", true)
            .bool("Anti break", true)
            .bool("Auto cast", true)
            .num("Cast delay (ticks)", 14.0, 0.0, 60.0, 1.0)
            .num("Catch delay (ticks)", 6.0, 0.0, 60.0, 1.0),
        Module::new("Auto Gap", Player, "Eats golden apples when your health is low or an effect is missing.")
            .bool("Allow egap", true)
            .bool("Always", false)
            .bool("Potions regeneration", false)
            .bool("Potions fire resistance", true)
            .bool("Potions absorption", false)
            .bool("Health enabled", true)
            .num("Health threshold", 20.0, 1.0, 40.0, 1.0),
        Module::new("Auto Steal", Player, "Takes everything out of a chest, barrel, shulker box or hopper as you open it.")
            .num("Items per tick", 3.0, 1.0, 27.0, 1.0)
            .num("Delay (ticks)", 0.0, 0.0, 10.0, 1.0),
        Module::new("Inventory Cleaner", Player, "Drops the listed items from your inventory while it is open.")
            .text("Items", "rotten_flesh,spider_eye,poisonous_potato,gravel,cobblestone,netherrack,cobbled_deepslate,andesite,diorite,granite,tuff")
            .num("Delay (ticks)", 1.0, 0.0, 10.0, 1.0),
        Module::new("Fast Use", Player, "Lets you use items at very high speeds.")
            .pick("Mode", &["All", "Some"])
            .text("Items", "ender_pearl,experience_bottle,snowball,egg")
            .bool("Blocks", false)
            .num("Cooldown (ticks)", 0.0, 0.0, 4.0, 1.0),
        Module::new("Spam", Player, "Spams chat messages. Separate several messages with |.")
            .text("Messages", "Sunset on top!")
            .num("Delay (ticks)", 20.0, 1.0, 400.0, 1.0)
            .bool("Randomise", false)
            .bool("Bypass", false)
            .num("Bypass length", 16.0, 4.0, 32.0, 1.0),
        Module::new("Reach", Player, "Longer arms for blocks (on a vanilla server only about 4 blocks more, and only for chests and vehicles).")
            .num("Extra block reach", 1.0, 0.0, 6.0, 0.5),
        Module::new("Air Place", Player, "Right click places the block you hold where the crosshair points, with nothing to place it against.")
            .num("Range", 5.0, 2.0, 6.0, 0.5),
        Module::new("Anchor", Category::Movement, "Stops your movement over a hole so you drop into it.")
            .num("Max height", 10.0, 1.0, 20.0, 1.0)
            .num("Min pitch", 0.0, -90.0, 90.0, 5.0)
            .bool("Pull", false)
            .num("Pull speed", 0.3, 0.05, 1.0, 0.05),
    ]
}

#[derive(Default)]
struct Ps {
    fish_state: u8,
    cast_left: u32,
    catch_left: u32,
    gap_eating: bool,
    gap_prev: usize,
    spam_timer: u32,
    spam_index: usize,
    rng: u64,
    afk_jump: bool,
    afk_sneak: u32,
    afk_strafe: i32,
    afk_right: bool,
    afk_key: Option<KeyCode>,
    afk_yaw: Option<f32>,
    afk_message: i32,
    afk_index: usize,
    steal_wait: u32,
    steal_id: i32,
    steal_last: usize,
    steal_same: u32,
    steal_done: i32,
    clean_wait: u32,
    // No Fall's Place: what it put down to land in, the item, the ticks since, the ticks you have stood still, and
    // whether the server's answer (the water there) has come.
    fall_placed: Option<([i32; 3], &'static str, u32, u32, bool)>,
}

static PS: Mutex<Option<Ps>> = Mutex::new(None);

fn ps<R>(f: impl FnOnce(&mut Ps) -> R) -> R {
    let mut g = PS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Ps::default))
}

fn rand(n: usize) -> usize {
    ps(|p| {
        p.rng = p.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407 + n as u64);
        ((p.rng >> 33) as usize) % n.max(1)
    })
}

// Reach's extra, read by the reach hook (f64 bits; None when off).
static REACH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

pub(super) fn reach_extra() -> Option<f64> {
    let bits = REACH.load(std::sync::atomic::Ordering::Relaxed);
    (bits != u64::MAX).then(|| f64::from_bits(bits))
}

// Right mouse button down while playing: Air Place.
pub(super) fn right_click(game: &mut Game<'_>) {
    let Some(m) = super::sunset::with(|s| s.module("Air Place").filter(|m| m.on).cloned()) else {
        return;
    };
    if !in_world(game) || game.pick().is_some() || game.entity_targeted() {
        return;
    }
    let held = game.held().is_some_and(|s| game.res.items().get(&s.item).is_some_and(|i| i.block.is_some()));
    if !held {
        return;
    }
    let (Some(eye), yaw, pitch) = (game.eye_position(), f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians()) else {
        return;
    };
    let d = m.n("Range");
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let at = [
        (eye[0] + look[0] * d).floor() as i32,
        (eye[1] + look[1] * d).floor() as i32,
        (eye[2] + look[2] * d).floor() as i32,
    ];
    let slot = game.selected;
    place_block(game, at, slot, false, true);
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    REACH.store(
        get("Reach").map_or(u64::MAX, |m| m.n("Extra block reach").to_bits()),
        std::sync::atomic::Ordering::Relaxed,
    );
    // These work on the inventory screen, which in_world leaves out.
    if game.remote.is_some() && game.player.is_some() {
        if let Some(m) = get("Auto Steal") {
            auto_steal(game, m);
        }
        if let Some(m) = get("Inventory Cleaner") {
            inventory_cleaner(game, m);
        }
    }
    if !in_world(game) {
        return;
    }
    if let Some(m) = get("Auto Fish") {
        auto_fish(game, m);
    } else {
        ps(|p| p.fish_state = 0);
    }
    if let Some(m) = get("Auto Gap") {
        auto_gap(game, m);
    } else if ps(|p| std::mem::take(&mut p.gap_eating)) {
        let _ = game.play_mouse(MouseButton::Right, false);
    }
    if let Some(m) = get("Fast Use") {
        fast_use(game, m);
    }
    if let Some(m) = get("Anti AFK") {
        anti_afk(game, m);
    } else {
        ps(|p| {
            p.afk_yaw = None;
            p.afk_message = 0;
        });
    }
    if let Some(m) = get("Spam") {
        spam(game, m);
    } else {
        ps(|p| p.spam_timer = 0);
    }
    if let Some(m) = get("Anchor") {
        anchor(game, m);
    }
    match get("No Fall") {
        Some(m) => no_fall(game, m),
        None => ps(|p| p.fall_placed = None),
    }
}

// ---- Auto Fish

fn rod_ok(game: &Game<'_>, slot: usize, anti_break: bool) -> Option<u32> {
    let s = game.inventory.get(slot)?.as_ref()?;
    if short(&s.item) != "fishing_rod" {
        return None;
    }
    let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
    if anti_break && max > 0 && s.damage + 1 >= max {
        return None;
    }
    let lvl = |e: &str| s.enchantment_level(e);
    Some(lvl("minecraft:luck_of_the_sea") + lvl("minecraft:lure") + lvl("minecraft:mending") + lvl("minecraft:unbreaking"))
}

fn use_rod(game: &mut Game<'_>, m: &Module) -> bool {
    let anti = m.b("Anti break");
    let selected = game.selected;
    let mut best: Option<(usize, u32)> = rod_ok(game, selected, anti).map(|s| (selected, s));
    for i in 0..9 {
        if i == selected {
            continue;
        }
        if let Some(score) = rod_ok(game, i, anti) {
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((i, score));
            }
        }
    }
    let Some((slot, _)) = best else {
        return false;
    };
    if slot != selected {
        if !m.b("Auto switch") {
            return false;
        }
        hold(game, slot);
    }
    let _ = game.play_mouse(MouseButton::Right, true);
    let _ = game.play_mouse(MouseButton::Right, false);
    ps(|p| p.cast_left = m.n("Cast delay (ticks)") as u32);
    true
}

fn auto_fish(game: &mut Game<'_>, m: &Module) {
    match super::fishing::sunset_fishing::state(game) {
        None => {
            // Idle: cast again after the delay.
            ps(|p| p.fish_state = 0);
            if !m.b("Auto cast") {
                return;
            }
            if ps(|p| {
                if p.cast_left > 0 {
                    p.cast_left -= 1;
                    true
                } else {
                    false
                }
            }) {
                return;
            }
            if use_rod(game, m) {
                ps(|p| p.fish_state = 1);
            }
        }
        Some((bobbing, biting, hooked)) => {
            let state = ps(|p| p.fish_state);
            match state {
                0 => ps(|p| p.fish_state = 1),
                1 => {
                    if hooked {
                        if use_rod(game, m) {
                            ps(|p| p.fish_state = 0);
                        }
                    } else if bobbing && biting {
                        ps(|p| {
                            p.catch_left = m.n("Catch delay (ticks)") as u32;
                            p.fish_state = 2;
                        });
                    }
                }
                _ => {
                    let wait = ps(|p| {
                        if p.catch_left > 0 {
                            p.catch_left -= 1;
                            true
                        } else {
                            false
                        }
                    });
                    if hooked || !wait {
                        if use_rod(game, m) {
                            ps(|p| p.fish_state = 0);
                        }
                    }
                }
            }
        }
    }
}

// ---- Auto Gap

fn gap_slot(game: &Game<'_>, m: &Module, need_egap: bool) -> Option<usize> {
    let plain = if need_egap { None } else { find_hotbar(game, |n| n == "golden_apple") };
    plain.or_else(|| if m.b("Allow egap") || need_egap { find_hotbar(game, |n| n == "enchanted_golden_apple") } else { None })
}

fn auto_gap(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let effects = &game.potions.effects;
    let missing = |name: &str| effects.amplifier(name).is_none();
    let mut need_egap = false;
    let mut should = m.b("Always");
    if !should && m.b("Potions regeneration") && missing("regeneration") {
        should = true;
    }
    if !should && m.b("Potions fire resistance") && missing("fire_resistance") {
        should = true;
        need_egap = true;
    }
    if !should && m.b("Potions absorption") && missing("absorption") {
        should = true;
        need_egap = true;
    }
    if !should && m.b("Health enabled") {
        let health = (p.health.health() + p.health.absorption()).round();
        should = f64::from(health) < m.n("Health threshold");
    }
    let eating = ps(|s| s.gap_eating);
    if eating {
        if should {
            match gap_slot(game, m, need_egap) {
                Some(slot) => {
                    hold(game, slot);
                    if game.using.is_none() {
                        let _ = game.play_mouse(MouseButton::Right, true);
                    }
                }
                None => stop_eating(game),
            }
        } else {
            stop_eating(game);
        }
    } else if should {
        if let Some(slot) = gap_slot(game, m, need_egap) {
            let prev = game.selected;
            ps(|s| {
                s.gap_prev = prev;
                s.gap_eating = true;
            });
            hold(game, slot);
            let _ = game.play_mouse(MouseButton::Right, true);
        }
    }
}

fn stop_eating(game: &mut Game<'_>) {
    let _ = game.play_mouse(MouseButton::Right, false);
    let prev = ps(|s| {
        s.gap_eating = false;
        s.gap_prev
    });
    hold(game, prev);
}

// ---- Fast Use

fn fast_use(game: &mut Game<'_>, m: &Module) {
    let item = game.held().map(|s| short(&s.item).to_owned());
    let applies = m.p("Mode") == "All"
        || item.as_ref().is_some_and(|i| {
            listed(m.t("Items"), i)
                || m.b("Blocks") && game.res.items().get(&format!("minecraft:{i}")).is_some_and(|inf| inf.block.is_some())
        });
    if applies && game.use_delay > m.n("Cooldown (ticks)") as u32 {
        game.use_delay = m.n("Cooldown (ticks)") as u32;
    }
}

// ---- Auto Steal and Inventory Cleaner

// How many slots a server's container menu has, for the ones that are just a grid of items.
fn container_size(menu: &str) -> Option<usize> {
    let name = menu.strip_prefix("minecraft:").unwrap_or(menu);
    match name {
        "shulker_box" | "generic_9x3" => Some(27),
        "hopper" => Some(5),
        "generic_3x3" => Some(9),
        _ => name.strip_prefix("generic_9x")?.parse::<usize>().ok().map(|rows| rows * 9),
    }
}

fn auto_steal(game: &mut Game<'_>, m: &Module) {
    use horizon_client::menu::Input::QuickMove;
    if game.screen != Screen::Inventory {
        // Closed: the next chest starts afresh.
        ps(|p| p.steal_done = i32::MIN);
        return;
    }
    // A chest of your own world, or the menu a server opened.
    let (id, size) = match game.container {
        Some(key) if matches!(game.container_kind(), Some(super::BlockEntity::Chest(_))) => {
            (key.0 ^ key.1.rotate_left(10) ^ key.2.rotate_left(20) | 1, game.container_size())
        }
        Some(_) => return,
        None => {
            let (id, menu) = game.remote_menu();
            match container_size(&menu) {
                Some(n) => (id, n),
                None => return,
            }
        }
    };
    if ps(|p| p.steal_done == id) || ps(|p| {
        p.steal_wait = p.steal_wait.saturating_sub(1);
        p.steal_wait > 0
    }) {
        return;
    }
    let items: Vec<usize> = (0..size).filter(|&i| game.slot_mut(300 + i).is_some()).collect();
    // Done when it is empty, or when taking stopped changing it (your inventory is full).
    let stuck = ps(|p| {
        p.steal_same = if p.steal_id == id && p.steal_last == items.len() { p.steal_same + 1 } else { 0 };
        (p.steal_id, p.steal_last) = (id, items.len());
        p.steal_same >= 6
    });
    if items.is_empty() || stuck {
        ps(|p| p.steal_done = id);
        return;
    }
    let ops: Vec<_> = items.iter().take(m.n("Items per tick") as usize).map(|&i| (Some(300 + i), 0, QuickMove)).collect();
    game.menu_ops(&ops);
    ps(|p| p.steal_wait = m.n("Delay (ticks)") as u32);
}

fn inventory_cleaner(game: &mut Game<'_>, m: &Module) {
    use horizon_client::menu::Input::Throw;
    if game.screen != Screen::Inventory || game.remote_menu().1 != "inventory" {
        return;
    }
    if ps(|p| {
        p.clean_wait = p.clean_wait.saturating_sub(1);
        p.clean_wait > 0
    }) {
        return;
    }
    let junk = (0..36).find(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| listed(m.t("Items"), short(&s.item))));
    if let Some(i) = junk {
        game.menu_ops(&[(Some(i), 1, Throw)]);
        ps(|p| p.clean_wait = m.n("Delay (ticks)") as u32 + 1);
    }
}

// ---- Anti AFK (Meteor's AntiAFK)

// The keys it holds this tick: a jump now and then, sneaking, and strafing left and right.
pub(super) fn afk_keys(m: &Module) -> Vec<KeyCode> {
    let mut keys = Vec::new();
    if m.b("Jump") {
        // rand takes the same lock as ps: roll first, or the game freezes.
        let roll = rand(99) == 0;
        let jump = ps(|p| {
            p.afk_jump = !p.afk_jump && roll;
            p.afk_jump
        });
        if jump {
            keys.push(KeyCode::Space);
        }
    }
    if m.b("Sneak") {
        let sneak = ps(|p| {
            let held = p.afk_sneak < m.n("Sneak time") as u32;
            p.afk_sneak += 1;
            held
        });
        if sneak {
            keys.push(KeyCode::ShiftLeft);
        } else if rand(99) == 0 {
            ps(|p| p.afk_sneak = 0);
        }
    }
    if m.b("Strafe") {
        let key = ps(|p| {
            p.afk_strafe -= 1;
            if p.afk_strafe <= 0 {
                p.afk_key = Some(if p.afk_right { KeyCode::KeyD } else { KeyCode::KeyA });
                p.afk_right = !p.afk_right;
                p.afk_strafe = 20;
            }
            p.afk_key
        });
        keys.extend(key);
    }
    keys
}

fn anti_afk(game: &mut Game<'_>, m: &Module) {
    if !in_world(game) {
        return;
    }
    if m.b("Swing") && rand(99) == 0 {
        game.swing_arm();
        game.send(S::Punch(Empty));
    }
    if m.b("Spin") {
        let yaw = ps(|p| {
            let y = p.afk_yaw.unwrap_or(game.yaw) + m.n("Speed") as f32;
            p.afk_yaw = Some(y);
            y
        });
        if m.p("Spin mode") == "Client" {
            game.yaw = yaw;
        } else {
            let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
            game.send(S::MovePlayerRot(play::MovePlayerRot {
                yaw,
                pitch: m.n("Pitch") as f32,
                flags: u8::from(ground),
            }));
        }
    }
    if m.b("Send messages") {
        let messages: Vec<&str> = m.t("Messages").split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
        if messages.is_empty() {
            return;
        }
        let due = ps(|p| {
            p.afk_message -= 1;
            p.afk_message <= 0
        });
        if due {
            let i = if m.b("Random") {
                rand(messages.len())
            } else {
                ps(|p| {
                    p.afk_index = (p.afk_index + 1) % messages.len();
                    p.afk_index
                })
            };
            game.send_chat(messages[i]);
            ps(|p| p.afk_message = m.n("Delay (seconds)") as i32 * 20);
        }
    }
}

// ---- Spam

fn spam(game: &mut Game<'_>, m: &Module) {
    if game.remote.is_none() {
        toggle_off(game, "Spam");
        return;
    }
    if ps(|p| {
        if p.spam_timer > 0 {
            p.spam_timer -= 1;
            true
        } else {
            false
        }
    }) {
        return;
    }
    let messages: Vec<&str> = m.t("Messages").split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
    if messages.is_empty() {
        return;
    }
    let i = if m.b("Randomise") {
        rand(messages.len())
    } else {
        ps(|p| {
            let i = p.spam_index % messages.len();
            p.spam_index = i + 1;
            i
        })
    };
    let mut text = messages[i].to_owned();
    if m.b("Bypass") {
        const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        text.push(' ');
        for _ in 0..m.n("Bypass length") as usize {
            text.push(LETTERS[rand(LETTERS.len())] as char);
        }
    }
    text.truncate(256);
    game.send_chat(&text);
    ps(|p| p.spam_timer = m.n("Delay (ticks)") as u32);
}

// ---- Anchor

fn hole_block(game: &Game<'_>, p: [i32; 3]) -> bool {
    let n = facts(game, p).name.clone();
    matches!(short(&n), "bedrock" | "obsidian" | "crying_obsidian")
}

fn is_hole(game: &Game<'_>, x: i32, y: i32, z: i32) -> bool {
    hole_block(game, [x, y - 1, z])
        && hole_block(game, [x + 1, y, z])
        && hole_block(game, [x - 1, y, z])
        && hole_block(game, [x, y, z + 1])
        && hole_block(game, [x, y, z - 1])
}

fn anchor(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    if game.pitch < m.n("Min pitch") as f32 {
        return;
    }
    let (px, py, pz) = (p.position.x, p.position.y, p.position.z);
    let (x, mut y, z) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
    if is_hole(game, x, y, z) {
        return;
    }
    let mut found = None;
    for _ in 0..m.n("Max height") as i32 {
        y -= 1;
        if y <= -64 || !(is_air(game, [x, y, z]) || !facts(game, [x, y, z]).full_collision) {
            break;
        }
        if is_hole(game, x, y, z) {
            found = Some([f64::from(x) + 0.5, f64::from(z) + 0.5]);
            break;
        }
    }
    if let Some([hx, hz]) = found {
        let pull = if m.b("Pull") { m.n("Pull speed") } else { 0.0 };
        if let Some(p) = game.player.as_mut() {
            p.velocity.x = (hx - px).clamp(-0.05, 0.05);
            p.velocity.z = (hz - pz).clamp(-0.05, 0.05);
            p.velocity.y -= pull;
        }
    }
}

#[allow(dead_code)]
fn unused(_: Screen) {}

// ---- No Fall (Meteor's NoFall): Air place and Place. Packet and No ground change the move packets (runtime.rs's
// outgoing). This runs after the tick, once its move packet has gone, where Meteor uses the item.

// Place's choices: the item, and whether it is a bucket (used, and picked back up) or a block (placed).
const FALL_ITEMS: [(&str, &str, bool); 5] = [
    ("Bucket", "water_bucket", true),
    ("Powder snow", "powder_snow_bucket", true),
    ("Hay bale", "hay_block", false),
    ("Cobweb", "cobweb", false),
    ("Slime block", "slime_block", false),
];

fn no_fall(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    if game.creative || p.flying || (m.b("Pause on mace") && game.held().is_some_and(|s| short(&s.item) == "mace")) {
        return;
    }
    match m.p("Mode") {
        "Air place" => fall_air_place(game, m),
        "Place" => fall_place(game, m),
        _ => {}
    }
}

// Air place: a block under you once the fall would hurt (Before damage) or kill (Before death).
fn fall_air_place(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let fallen = p.fall_distance;
    let health = f64::from(p.health.health() + p.health.absorption());
    let due = if m.p("Air place mode") == "Before damage" { fallen > 2.0 } else { fallen > health.max(2.0) };
    if !due {
        return;
    }
    let Some(slot) = (0..9).find(|&i| super::sunset::item_info(game, i).is_some_and(|info| info.block.is_some())) else {
        return;
    };
    if m.b("Anchor") {
        center_player(game);
    }
    if let Some(at) = block_pos(game) {
        place_block(game, [at[0], at[1] - 1, at[2]], slot, m.b("Rotate"), true);
    }
}

// Place: water (or what Placed item says) where the fall will land, a moment before it does; once you have stood
// still for two ticks, the water or powder snow goes back in the bucket.
fn fall_place(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (vy, fallen, still) = (p.velocity.y, p.fall_distance, p.fall_distance == 0.0 && p.ground.on_ground);
    if let Some((at, item, timer, stood, seen)) = ps(|s| s.fall_placed) {
        let stood = if still { (stood + 1).min(2) } else { 0 };
        let there = facts(game, at);
        let present = if item == "water_bucket" { there.fluid_level == 8 && there.name.ends_with("water") } else { there.name.ends_with("powder_snow") };
        // Gone once it was there (it was picked up, or flowed away), or never there in a second.
        let mut done = (seen && !present) || timer > 20;
        if stood >= 2 && present {
            if let (Some(slot), Some(eye)) = (find_hotbar(game, |n| n == "bucket"), game.eye_position()) {
                let (yaw, pitch) = look_at(eye, center_of(at));
                use_held(game, slot, yaw, pitch);
            }
            done = true;
        }
        ps(|s| s.fall_placed = (!done).then_some((at, item, timer + 1, stood, seen || present)));
        return;
    }
    if vy >= 0.0 || (fallen <= 3.0 && vy > -0.6) || p.fluids.was_touching_water {
        return;
    }
    let chosen = m.p("Placed item");
    // Water boils away in the Nether: powder snow instead.
    let chosen = if chosen == "Bucket" && game.dim.id() == "minecraft:the_nether" { "Powder snow" } else { chosen };
    let Some(&(_, item, bucket)) = FALL_ITEMS.iter().find(|i| i.0 == chosen) else {
        return;
    };
    let Some(slot) = find_hotbar(game, |n| n == item) else {
        return;
    };
    if m.b("Anchor") {
        center_player(game);
    }
    let (Some((ground, point)), Some(eye)) = (landing(game), game.eye_position()) else {
        return;
    };
    // The use reaches as far as a click does (4.5 blocks); further up, it waits for a later tick.
    if dist(eye, point) > 4.5 {
        return;
    }
    let above = [ground[0], ground[1] + 1, ground[2]];
    if bucket {
        let (yaw, pitch) = look_at(eye, point);
        use_held(game, slot, yaw, pitch);
        ps(|s| s.fall_placed = Some((above, item, 0, 0, false)));
    } else {
        place_block(game, above, slot, m.b("Rotate"), true);
    }
}

// Uses the item in a hotbar slot looking at (yaw, pitch), as a right click into the air does, then holds what you
// held before.
fn use_held(game: &mut Game<'_>, slot: usize, yaw: f32, pitch: f32) {
    let before = game.selected;
    hold(game, slot);
    let seq = sequence(game);
    game.send(S::UseItem(play::UseItem { hand: 0, sequence: seq, yaw, pitch }));
    game.swing_arm();
    hold(game, before);
}

// Where the fall lands within the next 8 ticks (Meteor's findLandingSurface, straight down): the block it lands on and
// the point on its top below you; nothing if it lands in water or later.
fn landing(game: &Game<'_>) -> Option<([i32; 3], [f64; 3])> {
    let p = game.player.as_ref()?;
    let (x, z) = (p.position.x, p.position.z);
    let (bx, bz) = (x.floor() as i32, z.floor() as i32);
    let feet = p.position.y.floor() as i32;
    let below = (0..64).map(|d| feet - 1 - d).find(|&y| !is_air(game, [bx, y, bz]))?;
    let ground = [bx, below, bz];
    if facts(game, ground).fluid_level > 0 || !facts(game, ground).full_collision {
        return None;
    }
    let top = f64::from(below + 1);
    let (mut y, mut vy) = (p.position.y, p.velocity.y);
    for _ in 0..8 {
        y += vy;
        vy = (vy - 0.08) * 0.98;
        if y <= top {
            return Some((ground, [x, top, z]));
        }
    }
    None
}
