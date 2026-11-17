// A press on Client Sort's buttons, instead of Horizon's sort button.
fn sunset_sort_press(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<bool> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if crate::game::sunset_extras::on() {
        return std::ops::ControlFlow::Break(crate::game::extras::sorting::sunset_sort_buttons::press(game));
    }
    std::ops::ControlFlow::Continue(())
}
