// Shared scheduling for the explosive modules. One deferred inventory/action
// owner at a time; neither module sends combat actions from a render hook.
use super::{Game, Screen};
use super::sunset::{self, Module};
use std::sync::atomic::Ordering;
pub(super) use super::sunset_explosive_choice::Rating;
use super::sunset_explosive_choice::prefer_first;

pub(super) fn can_act(game: &Game<'_>) -> bool {
    game.native_window() && game.native_on(sunset::ID)
        && game.native_playing() && game.screen == Screen::Playing
        && game.remote.is_some() && !sunset::gui_open()
        && !game.spectator() && !game.creative && game.selected < 9
        && game.using.is_none() && game.mining.is_none()
        && super::remote::sunset_remote::breaking(game).is_none()
        && !super::sunset_player::gap_eating()
        && !game.left && !game.right
        && super::sunset_util::inventory_idle(game)
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && sunset::CRIT_STAGE.load(Ordering::Relaxed) == 0
        && super::sunset_combat_control::owner().is_none_or(|o| o == "Crystal Aura" || o == "Anchor Aura")
        && !sunset::with(|s| ["Blink", "Nuker", "Scaffold"]
            .iter().any(|name| s.is_on(name)))
}

pub(super) fn tick(game: &mut Game<'_>, modules: &[Module], ticks: u64) {
    let crystal = modules.iter().find(|m| m.name == "Crystal Aura" && m.on);
    let anchor = modules.iter().find(|m| m.name == "Anchor Aura" && m.on);
    if crystal.is_none() { super::sunset_crystal_aura::reset(); }
    if anchor.is_none() { super::sunset_anchor_aura::reset(); }
    if !can_act(game) {
        if crystal.is_some() { super::sunset_crystal_aura::suspend(); }
        if anchor.is_some() { super::sunset_anchor_aura::reset(); }
        return;
    }
    // Prepare each at most once, then claim only the chosen action. A pending
    // server acknowledgement is not a new inventory/rotation owner.
    let c = super::sunset_crystal_aura::prepare(game, crystal, ticks).filter(|r| r.valid());
    let a = anchor.and_then(|m| super::sunset_anchor_aura::prepare(game, m, ticks)).filter(|r| r.valid());
    let crystal_first = match (c, a) {
        (Some(c), Some(a)) => prefer_first(c, a, ticks % 2 == 0),
        (Some(_), None) => true,
        _ => false,
    };
    if crystal_first {
        if let Some(m) = crystal { super::sunset_crystal_aura::execute(game, m, ticks); }
    } else if a.is_some() {
        if let Some(m) = anchor { super::sunset_anchor_aura::execute(game, m, ticks); }
    }
}
