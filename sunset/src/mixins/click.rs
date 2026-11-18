// A row of Sunset's page pressed.
fn sunset_click(game: &mut Game<'_>, i: usize, j: usize) -> ::core::ops::ControlFlow<()> {
    if crate::game::sunset_runtime_menu::mod_index(game) != Some(i) {
        return ::core::ops::ControlFlow::Continue(());
    }
    crate::game::sunset_runtime_menu::page_pressed(game, j);
    ::core::ops::ControlFlow::Break(())
}
