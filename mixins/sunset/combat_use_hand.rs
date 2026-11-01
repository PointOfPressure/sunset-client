// Friend Guard: a spear's use (its charge) that would run through a friend does not start.
fn sunset_combat_use_hand(
    game: &crate::game::Game<'_>,
    hand: horizon_client::first_person::Hand,
) -> std::ops::ControlFlow<bool> {
    let slot = if hand == horizon_client::first_person::Hand::Off { crate::game::sunset_util::OFFHAND } else { game.selected };
    if game.stack_in(slot).is_some_and(|s| horizon_client::spear::Kinetic::of(&s.item).is_some())
        && crate::game::sunset_melee::block_stab(game)
    {
        std::ops::ControlFlow::Break(true)
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
