// Chest Swap's Take off: the elytra on (from anywhere in the inventory, a chestplate taken off to where it was), a
// jump (Auto jump) and the glide opened with a jump press in the air, a firework rocket (Firework: one from the main
// inventory is swapped into the hotbar first and back after), then with Auto disable the chestplate back on after
// Fly time and the module off; without it the module stays on (Firework on space fires one at each Space press
// while you glide) until you turn it off, which swaps back. One step a tick at most, Delay apart, at the start of a
// tick; clicks go with no screen open, as Chest Swap's do.
use super::Game;
use super::sunset::{Module, hold, say, toggle_named, wear, with};
use super::sunset_util as util;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;

const NAME: &str = "Chest Swap";

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Stage {
    #[default]
    Idle,
    Arm,
    Equip,
    Rocket,
    Glide,
    Boost,
    Fly,
}

#[derive(Default)]
struct Takeoff {
    stage: Stage,
    next: u64,
    // Where the chestplate went; the rocket's main inventory slot and the hotbar slot it was swapped into.
    chest_back: Option<usize>,
    rocket_back: Option<(usize, usize)>,
    // The hotbar slot from before a rocket was held, and since when you glide.
    slot_back: Option<usize>,
    glide_since: Option<u64>,
    glide_try: u64,
    jump: bool,
    // Firework on space: a press waiting, and the tick the next may go.
    space: bool,
    space_ready: u64,
    roll: u64,
}

static TAKEOFF: Mutex<Option<Takeoff>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut Takeoff) -> R) -> R {
    f(TAKEOFF.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Takeoff::default))
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on && m.p("Mode") == "Take off").cloned())
}

fn item_at(game: &Game<'_>, item: &str) -> Option<usize> {
    (0..36).find(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| util::short(&s.item) == item))
}

fn worn(game: &Game<'_>) -> bool {
    game.armor[1].as_ref().is_some_and(|s| util::short(&s.item) == "elytra")
}

fn gliding(game: &Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|p| p.fall_flying)
}

fn ms_ticks(ms: f64) -> u64 {
    (ms / 50.0).round().max(0.0) as u64
}

// Each step of the routine waits Delay (ticks) after the last.
fn after(game: &Game<'_>, m: &Module) -> u64 {
    game.ticks + (m.n("Delay (ticks)") as u64).max(1)
}

// The chestplate back on, the rocket back where it came from.
fn undo(game: &mut Game<'_>) {
    let (chest, rocket, slot) = st(|s| (s.chest_back.take(), s.rocket_back.take(), s.slot_back.take()));
    if !super::sunset::inventory_idle(game) {
        return;
    }
    if let Some(b) = slot.filter(|&b| b != game.selected) {
        hold(game, b);
    }
    if let Some(i) = chest.filter(|_| worn(game)) {
        wear(game, i, 1);
    }
    if let Some((from, to)) = rocket {
        game.remote_menu_ops(&[(Some(from), to as i32, horizon_client::menu::Input::Swap)]);
    }
}

pub(super) fn key(key: &str, pressed: bool) {
    if pressed && key == "key.keyboard.space" && module().is_some_and(|m| m.b("Firework on space")) {
        st(|s| s.space = s.stage == Stage::Fly);
    }
}

// A rocket used from the hotbar: held now, the slot from before back a step later.
fn rocket(game: &mut Game<'_>) -> bool {
    let Some(slot) = (0..9).find(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| util::short(&s.item) == "firework_rocket")) else {
        return false;
    };
    if game.selected != slot {
        let before = game.selected;
        st(|s| {
            s.slot_back.get_or_insert(before);
        });
        hold(game, slot);
    }
    let sequence = util::sequence(game);
    let yaw = super::sunset::wire_yaw(game, game.yaw);
    game.send(S::UseItem(play::UseItem { hand: 0, sequence, yaw, pitch: game.pitch }));
    game.swing_arm();
    true
}

pub(super) fn pre_input(game: &mut Game<'_>) {
    if st(|s| std::mem::take(&mut s.jump)) {
        super::sunset_melee::press_jump(game);
    }
}

pub(super) fn pre_tick(game: &mut Game<'_>, busy: bool) {
    let Some(m) = module().filter(|_| util::in_world(game)) else {
        if st(|s| s.stage != Stage::Idle) {
            undo(game);
            st(|s| *s = Takeoff { roll: s.roll, ..Takeoff::default() });
        }
        return;
    };
    let now = game.ticks;
    if st(|s| s.stage == Stage::Idle) {
        st(|s| {
            s.stage = Stage::Arm;
            s.next = now;
        });
    }
    if busy || now < st(|s| s.next) {
        return;
    }
    // A slot borrowed for a rocket comes back first.
    if let Some(b) = st(|s| s.slot_back.take()) {
        if b != game.selected {
            hold(game, b);
            st(|s| s.next = after(game, &m));
            return;
        }
    }
    let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
    let stage = st(|s| s.stage);
    match stage {
        Stage::Idle => {}
        Stage::Arm => {
            if !m.b("Require land") || ground {
                st(|s| s.stage = Stage::Equip);
            }
        }
        Stage::Equip => {
            let wants_rocket = m.b("Firework");
            if wants_rocket && item_at(game, "firework_rocket").is_none() {
                say(game, "Take off: no firework rockets.");
                return toggle_named(game, NAME, false);
            }
            if !worn(game) {
                let Some(i) = item_at(game, "elytra") else {
                    say(game, "Take off: no elytra in your inventory.");
                    return toggle_named(game, NAME, false);
                };
                // A click while moving is what Grim's MultiActionsC flags: the elytra waits until you stand still.
                if !super::sunset::inventory_idle(game) || !super::remote::sunset_combat_remote::quiet(game) {
                    return;
                }
                let had_chest = game.armor[1].is_some();
                wear(game, i, 1);
                st(|s| s.chest_back = had_chest.then_some(i));
            }
            st(|s| {
                s.stage = if wants_rocket { Stage::Rocket } else { Stage::Glide };
                s.next = after(game, &m);
            });
        }
        Stage::Rocket => {
            // A rocket only in the main inventory: swapped into an empty hotbar slot (or the held one).
            let in_hotbar = (0..9).any(|i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| util::short(&s.item) == "firework_rocket"));
            if !in_hotbar && let Some(from) = item_at(game, "firework_rocket") {
                if !super::sunset::inventory_idle(game) || !super::remote::sunset_combat_remote::quiet(game) {
                    return;
                }
                let to = (0..9).find(|&i| game.inventory.get(i).is_some_and(Option::is_none)).unwrap_or(game.selected);
                game.remote_menu_ops(&[(Some(from), to as i32, horizon_client::menu::Input::Swap)]);
                st(|s| s.rocket_back = Some((from, to)));
            }
            st(|s| {
                s.stage = Stage::Glide;
                s.next = after(game, &m);
            });
        }
        Stage::Glide => {
            if !worn(game) {
                return;
            }
            if gliding(game) {
                st(|s| {
                    s.glide_since = Some(now);
                    s.stage = if m.b("Firework") { Stage::Boost } else { Stage::Fly };
                });
            } else if ground {
                if m.b("Auto jump") {
                    st(|s| s.jump = true);
                }
            } else if now >= st(|s| s.glide_try) + 8 {
                st(|s| {
                    s.glide_try = now;
                    s.jump = true;
                });
            }
        }
        Stage::Boost => {
            rocket(game);
            st(|s| {
                s.stage = Stage::Fly;
                s.next = after(game, &m);
            });
        }
        Stage::Fly => {
            let since = st(|s| s.glide_since).unwrap_or(now);
            if m.b("Auto disable") && now >= since + ms_ticks(m.n("Fly time (ms)")) {
                undo(game);
                st(|s| *s = Takeoff { roll: s.roll, ..Takeoff::default() });
                return toggle_named(game, NAME, false);
            }
            if st(|s| std::mem::take(&mut s.space)) && gliding(game) && now >= st(|s| s.space_ready) && rocket(game) {
                let (lo, hi) = (m.n("Shortest space cooldown (ms)"), m.n("Longest space cooldown (ms)").max(m.n("Shortest space cooldown (ms)")));
                let r = st(|s| {
                    s.roll = (s.roll ^ now.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1;
                    s.roll ^= s.roll << 13;
                    s.roll ^= s.roll >> 7;
                    s.roll ^= s.roll << 17;
                    (s.roll >> 11) as f64 / (1u64 << 53) as f64
                });
                st(|s| {
                    s.space_ready = now + ms_ticks(lo + r * (hi - lo));
                    s.next = after(game, &m);
                });
            }
        }
    }
}
