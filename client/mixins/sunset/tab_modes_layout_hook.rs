// HUD's Tab game modes switched on or off (or Sunset stopped): the cached tab list is drawn again with the names as
// they are now.
fn sunset_tab_modes_layout(this: &mut TabList) {
    if crate::tab_list::sunset_tab_modes::changed(horizon_protocol::connection::sunset_lifecycle::RUNTIME.running()) {
        this.cache = None;
    }
}
