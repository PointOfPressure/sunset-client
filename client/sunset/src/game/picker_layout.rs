// The list picker's pieces (picker.rs), cut from its box by the UI layer (::sunset_core::ui). No game in here: the
// tests in client/tools/ui_tests.rs run it as it is.

pub(super) const ROW: f32 = 12.0;

// The picker's box, its list's area and its Done button.
pub(super) fn rect(w: f32, h: f32) -> [f32; 4] {
    let width = (w - 8.0).clamp(1.0, 300.0);
    let height = (h - 16.0).clamp(1.0, 280.0);
    [(w - width) / 2.0, ((h - height) / 2.0).max(0.0), width, height]
}

// The picker's pieces, cut from its box by the UI layer (::sunset_core::ui): the header, the search field, the list
// with a footer under it, and in the footer the note and the Done button. Drawing and clicking both read these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) frame: [f32; 4],
    pub(super) head: [f32; 4],
    pub(super) field: [f32; 4],
    pub(super) list: [f32; 4],
    pub(super) note: [f32; 4],
    pub(super) done: [f32; 4],
}

pub(super) fn layout(w: f32, h: f32) -> Layout {
    use ::sunset_core::ui::{Flow, inset};
    let frame = rect(w, h);
    let mut column = Flow::column(frame, 0.0);
    let head = column.take(22.0);
    let mut body = Flow::column(column.rest(), 0.0).padded([6.0, 0.0, 6.0, 0.0]);
    let field = body.take(16.0);
    body.skip(4.0);
    let mut foot = Flow::row(inset(body.take_end(22.0), [0.0, 3.0, 0.0, 4.0]), 6.0);
    let list = body.rest();
    let done = foot.take_end(56.0);
    Layout { frame, head, field, list, note: foot.rest(), done }
}

// The list's rows in its view, scrolled to offset (kept within what there is to scroll).
pub(super) fn scroll(list: [f32; 4], count: usize, offset: f32) -> ::sunset_core::ui::Scroll {
    ::sunset_core::ui::Scroll::new(list, ROW, count, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The layout layer cuts the same pieces the picker's hand-placed rectangles were.
    #[test]
    fn the_layout_is_where_the_pieces_were() {
        for (w, h) in [(427.0, 240.0), (640.0, 360.0), (320.0, 200.0), (960.0, 540.0), (200.0, 120.0)] {
            let l = layout(w, h);
            let r = rect(w, h);
            assert_eq!(l.frame, r);
            assert_eq!(l.head, [r[0], r[1], r[2], 22.0]);
            assert_eq!(l.field, [r[0] + 6.0, r[1] + 22.0, r[2] - 12.0, 16.0]);
            assert_eq!(l.list, [r[0] + 6.0, r[1] + 42.0, r[2] - 12.0, (r[3] - 64.0).max(0.0)], "{w} {h}");
            assert_eq!(l.done, [r[0] + r[2] - 62.0, r[1] + r[3] - 19.0, 56.0, 15.0]);
            assert_eq!(l.note, [r[0] + 6.0, r[1] + r[3] - 19.0, r[2] - 74.0, 15.0]);
        }
        // Rows: the one a click lands on is the one drawn there, scrolled or not, and none below the last.
        let l = layout(427.0, 240.0);
        let s = scroll(l.list, 40, 30.0);
        let (n, row) = s.seen().nth(2).unwrap();
        assert_eq!(n, 4);
        assert_eq!(s.row_at((row[0] + 5.0, row[1] + 6.0)), Some(4));
        assert_eq!(s.row_at((l.done[0] + 2.0, l.done[1] + 2.0)), None);
        assert_eq!(scroll(l.list, 2, 500.0).offset, 0.0);
        assert_eq!(scroll(l.list, 2, 0.0).row_at((l.list[0] + 5.0, l.list[1] + 30.0)), None);
        assert_eq!(scroll(l.list, 40, 9999.0).offset, 40.0 * ROW - l.list[3]);
    }
}
