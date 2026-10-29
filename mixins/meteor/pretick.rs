// Before each tick: keys held for the player, flight, step height.
fn meteor_pretick(game: &mut crate::game::Game<'_>) {
    crate::game::meteor::pre_tick(game);
}
