// Options > Mods' rows are `ModList::installed`, and the screen keeps indices into them: the open
// native mod pages on its stack and the native key waiting to be bound. A row removed while the
// screen may be open (Sunset's, after Panic) closes what pointed at it and moves the later ones up,
// so no page or key binding is left showing or changing another mod.
use super::{OptionsUi, Page};

pub fn removed(ui: &mut OptionsUi, row: usize) {
    if let Some(at) = ui.stack.iter().position(|p| *p == Page::NativeMod(row)) {
        // Back to the list the page was opened from; a page opened on its own leaves the list.
        ui.stack.truncate(at);
        if ui.stack.last() != Some(&Page::Mods) {
            ui.stack.push(Page::Mods);
        }
        ui.scroll = 0.0;
        ui.scrollbar_drag_y = None;
    }
    for page in &mut ui.stack {
        if let Page::NativeMod(i) = page
            && *i > row
        {
            *i -= 1;
        }
    }
    match ui.selected_native_key {
        Some((i, _)) if i == row => {
            ui.selected_native_key = None;
            if ui.selected_key == Some(super::NATIVE_KEY) {
                ui.selected_key = None;
            }
        }
        Some((i, k)) if i > row => ui.selected_native_key = Some((i - 1, k)),
        _ => {}
    }
    // Keyboard and pad focus name widgets of the page as it was drawn.
    if ui.page() == Page::Mods {
        ui.focus = None;
        ui.focus_from_keyboard = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removed_row_closes_its_page_and_moves_the_later_ones_up() {
        let mut ui = OptionsUi::default();
        ui.stack = vec![Page::Main, Page::Mods, Page::NativeMod(2)];
        ui.selected_key = Some(super::super::NATIVE_KEY);
        ui.selected_native_key = Some((2, 1));
        removed(&mut ui, 2);
        assert_eq!(ui.stack, [Page::Main, Page::Mods]);
        assert_eq!((ui.selected_key, ui.selected_native_key), (None, None));

        ui.stack = vec![Page::Main, Page::Mods, Page::NativeMod(3)];
        ui.selected_key = Some(super::super::NATIVE_KEY);
        ui.selected_native_key = Some((3, 0));
        removed(&mut ui, 1);
        assert_eq!(ui.stack, [Page::Main, Page::Mods, Page::NativeMod(2)]);
        assert_eq!(ui.selected_native_key, Some((2, 0)));
        assert!(ui.selected_key.is_some());

        // Opened straight on the page (`open_at`): the list takes its place.
        ui.stack = vec![Page::NativeMod(0)];
        removed(&mut ui, 0);
        assert_eq!(ui.stack, [Page::Mods]);

        // An earlier row's page is left alone.
        ui.stack = vec![Page::Main, Page::Mods, Page::NativeMod(0)];
        removed(&mut ui, 4);
        assert_eq!(ui.stack, [Page::Main, Page::Mods, Page::NativeMod(0)]);
    }
}
