// After each tick: the modules' work.
fn sunset_tick(game: &mut crate::game::Game<'_>) {
    crate::game::sunset::post_tick(game);
}
