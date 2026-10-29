// Before each tick: keys held for the player, flight, step height.
fn delta_pretick(game: &mut crate::game::Game<'_>) {
    crate::game::delta::pre_tick(game);
}
