// Before each tick: keys held for the player, flight, step height.
fn solstice_pretick(game: &mut crate::game::Game<'_>) {
    crate::game::solstice::pre_tick(game);
}
