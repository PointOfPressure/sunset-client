// After each tick: the modules' work.
fn sunset_tick(game: &mut crate::game::Game<'_>) {
    crate::game::sunset::post_tick(game);
    crate::game::remote::sunset_tab_marks::apply(game, ::sunset_core::lifecycle::RUNTIME.running());
    crate::game::sunset_client_spoofer::pack_tick(game);
}
