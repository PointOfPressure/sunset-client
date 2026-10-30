// Once a tick: the plugin's work.
fn hello_tick(game: &mut crate::game::Game<'_>) {
    crate::game::hello_plugin::tick(game);
}
