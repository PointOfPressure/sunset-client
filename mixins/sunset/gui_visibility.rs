// F1 keeps an open Sunset menu visible; outside it Horizon toggles normally.
fn sunset_gui_visibility(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<()> {
    if crate::game::sunset::gui_keep_visible(game) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
