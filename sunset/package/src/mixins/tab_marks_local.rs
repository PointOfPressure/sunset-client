// HUD's Tab game modes and Streamer Mode in single player: the integrated server's players as the tab list gets them.
fn sunset_tab_marks_local(value: Vec<TabPlayer>) -> Vec<TabPlayer> {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return value;
    }
    crate::game::remote::sunset_tab_marks::local(value)
}
