// The wheel stays off the hotbar while the click GUI is open.
fn solstice_wheel(claimed: &mut bool) {
    if crate::game::solstice::gui_open() {
        *claimed = true;
    }
}
