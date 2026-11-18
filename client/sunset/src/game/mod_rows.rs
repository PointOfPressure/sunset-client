// Options > Mods' rows are `ModList::installed`, and the screen keeps indices into them: the open native mod pages
// on its stack. A row removed while the screen may be open (Sunset's, after Panic) closes the page that pointed at it
// and moves the later ones up, so no page is left showing another mod. (The screen's own scroll, focus and waiting
// key are its private state on this game build; leaving the page resets them as the game does.)
use horizon_client::options_ui::{OptionsUi, Page};

pub fn removed(ui: &mut OptionsUi, row: usize) {
    if let Some(at) = ui.stack.iter().position(|p| *p == Page::NativeMod(row)) {
        // Back to the list the page was opened from; a page opened on its own leaves the list.
        ui.stack.truncate(at);
        if ui.stack.last() != Some(&Page::Mods) {
            ui.stack.push(Page::Mods);
        }
    }
    for page in &mut ui.stack {
        if let Page::NativeMod(i) = page
            && *i > row
        {
            *i -= 1;
        }
    }
}
