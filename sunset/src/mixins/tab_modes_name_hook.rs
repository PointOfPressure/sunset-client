// HUD's Tab game modes: a spectator's or creative player's name in the tab list gets its tag.
fn sunset_tab_modes_name(p: &TabPlayer, board: &Scoreboard) -> std::ops::ControlFlow<Text> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::game::sunset_tab_modes::marked(p, board) {
        Some(name) => std::ops::ControlFlow::Break(name),
        None => std::ops::ControlFlow::Continue(()),
    }
}
