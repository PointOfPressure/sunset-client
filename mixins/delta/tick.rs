// After each tick: the modules' work.
fn delta_tick(game: &mut crate::game::Game<'_>) {
    crate::game::delta::post_tick(game);
}
