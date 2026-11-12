// HUD's Tab game modes: a spectator's or creative player's name in the tab list gets its tag.
fn sunset_tab_modes_name(p: &TabPlayer, board: &Scoreboard) -> std::ops::ControlFlow<Text> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::tab_list::sunset_tab_modes::marked(p, board) {
        Some(name) => std::ops::ControlFlow::Break(name),
        None => std::ops::ControlFlow::Continue(()),
    }
}
