// After each tick: the modules' work.
fn solstice_tick(game: &mut crate::game::Game<'_>) {
    crate::game::solstice::post_tick(game);
}
