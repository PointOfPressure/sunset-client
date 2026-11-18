// Friend Guard: a spear's use (its charge) that would run through a friend does not start. Wind Jump notes a wind
// charge thrown.
fn sunset_combat_use_hand(
    game: &crate::game::Game<'_>,
    hand: horizon_client::first_person::Hand,
) -> std::ops::ControlFlow<bool> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if game.using.is_none() && horizon_client::motion::sunset_motion::no_slow_grim() {
        crate::game::sunset_render::no_slow_reset();
    }
    let slot = if hand == horizon_client::first_person::Hand::Off { crate::game::sunset_util::OFFHAND } else { game.selected };
    crate::game::sunset_duel::used(game, slot);
    if game.stack_in(slot).is_some_and(|s| horizon_client::spear::Kinetic::of(&s.item).is_some())
        && crate::game::sunset_melee::block_stab(game)
    {
        std::ops::ControlFlow::Break(true)
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
