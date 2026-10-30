// Before each tick: keys held for the player, flight, step height.
fn sunset_pretick(game: &mut crate::game::Game<'_>) {
    crate::game::sunset::pre_tick(game);
}
