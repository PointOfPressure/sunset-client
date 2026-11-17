use super::sunset::{self, Module};
use super::sunset_click_clock::{Clock, Config, Mode};
use super::sunset_rotation::Rng;
use super::Game;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use winit::event::MouseButton;

struct State { start: Instant, left: Clock, right: Clock, random: Rng }
impl Default for State {
    fn default() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        Self { start: Instant::now(), left: Clock::default(), right: Clock::default(), random: Rng::new(seed) }
    }
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
pub(super) fn reset() { *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None; }
pub(super) fn button(button: MouseButton) {
    if let Some(state) = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        match button { MouseButton::Left => state.left.reset(), MouseButton::Right => state.right.reset(), _ => {} }
    }
}
fn live(game: &Game<'_>) -> bool {
    game.native_on(sunset::ID) && game.native_playing() && !sunset::gui_open()
        && game.remote.is_some() && !game.spectator() && game.sleeping.is_none()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
}
// Menus can stop client ticks; a frame may only discard state, never click.
pub(super) fn frame(game: &Game<'_>) {
    if !live(game) || !sunset::with(|s| s.is_on("Auto Clicker")) { reset(); }
}
fn mode(m: &Module, right: bool) -> Mode {
    let smooth = m.p("CPS pattern") == "Smooth";
    if right && m.b("Right own CPS") { Mode::cps(m.n("Right min CPS"), m.n("Right max CPS"), smooth) }
    else if m.p("Mode") == "CPS" { Mode::cps(m.n("Min CPS"), m.n("Max CPS"), smooth) }
    else { Mode::Delay(m.n("Delay (ticks)").max(1.0) as u32) }
}
pub(super) fn tick(game: &mut Game<'_>) {
    if !live(game) || game.using.is_some() || !super::sunset_combat_control::available("Auto Clicker")
        || super::sunset_combat_macros::active() || sunset::strike_waiting() || super::sunset_melee::acted(game)
    { reset(); return; }
    let Some(m) = sunset::with(|s| s.module("Auto Clicker").filter(|m| m.on).cloned()) else { reset(); return; };
    let weapon = !m.b("Only with a weapon") || super::sunset_combat::holds_weapon(game);
    let entity_first = game.mob_hit().is_some_and(|(_, distance, _)| game.before_block(distance));
    let block = !entity_first && game.pick().is_some();
    let left = m.b("Left") && weapon && !(m.b("Stop on blocks") && block)
        && (game.left || game.physical_buttons.contains(&MouseButton::Left));
    let items = m.t("Right items").trim();
    let item_ok = items.is_empty() || game.held().is_some_and(|held| {
        super::sunset_util::listed(items, &held.item) || (super::sunset_util::listed(items, "blocks")
            && game.res.items().get(&held.item).is_some_and(|item| item.block.is_some()))
    });
    let other_right = sunset::with(|s| {
        let block_item = [game.held(), game.offhand.as_ref()].into_iter().flatten()
            .any(|held| game.res.items().get(&held.item).is_some_and(|item| item.block.is_some()));
        (block_item && s.is_on("Fast Place")) || s.module("Fast Use").is_some_and(|m|
            m.on && super::sunset_combat_inventory::thrown_in_hand(game, m))
    });
    let right = m.b("Right") && weapon && item_ok && !other_right
        && (!m.b("Right sneak only") || game.player.as_ref().is_some_and(|p| p.shift_key_down))
        && (game.right || game.physical_buttons.contains(&MouseButton::Right));
    let (click_left, click_right) = {
        let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let state = guard.get_or_insert_with(State::default);
        let now = state.start.elapsed().as_secs_f64();
        let (l_roll, r_roll) = (state.random.next(), state.random.next());
        (state.left.poll(now, game.ticks, Config { mode: mode(&m, false), start_delay: 0.0 }, left, l_roll),
         state.right.poll(now, game.ticks, Config { mode: mode(&m, true), start_delay: m.n("Right start delay (ms)") / 1000.0 }, right, r_roll))
    };
    // The held-use fallback otherwise imposes an extra 5 CPS floor even when
    // the requested right-click rate is lower. The initial real click and
    // item-use holds remain the engine's; only its repeat waits for our clock.
    if right { game.use_delay = game.use_delay.max(1); }
    if click_left {
        // An air miss installs a ten-tick delay. Clear it only for a due
        // held-button repeat. The handoff/owner checks above protect melee's
        // intentional wait, including a one-tick handoff.
        if !m.b("Respect miss cooldown") { game.clicks.miss_time = 0; }
        super::remote::sunset_combat_remote::click(game, true);
    }
    if click_right { super::remote::sunset_combat_remote::click(game, false); }
}
