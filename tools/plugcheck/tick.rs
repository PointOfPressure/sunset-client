// Once a tick: the test plugins' setup (their first tick) and nothing after.
fn plugcheck_tick(game: &mut crate::game::Game<'_>) {
    crate::game::plugcheck_plugin::tick(game);
}
