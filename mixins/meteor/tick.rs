// After each tick: the modules' work.
fn meteor_tick(game: &mut crate::game::Game<'_>) {
    crate::game::meteor::post_tick(game);
}
