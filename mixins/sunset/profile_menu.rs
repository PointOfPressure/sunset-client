// The saved-profile chooser. Disk I/O happens on open/load, never while drawing.
use super::{Game, sunset::{Shape, accent}};

const ROW: f32 = 18.0;
#[derive(Clone, Debug)]
pub(super) struct Menu {
    pub names: Vec<String>,
    pub error: Option<String>,
    focus: usize,
    start: usize,
}

pub(super) fn button(w: f32) -> [f32; 4] { [4.0, 14.0, (w * 0.3).clamp(90.0, 200.0), 14.0] }
fn frame(w: f32, h: f32, count: usize) -> [f32; 4] {
    let ww = (w - 16.0).min(300.0);
    let hh = (count.clamp(1, 10) as f32 * ROW + 58.0).min(h - 16.0);
    [(w - ww) / 2.0, (h - hh) / 2.0, ww, hh]
}
fn capacity(r: [f32; 4]) -> usize { ((r[3] - 58.0) / ROW).max(1.0) as usize }
fn inside(r: [f32; 4], at: (f32, f32)) -> bool { at.0 >= r[0] && at.0 < r[0]+r[2] && at.1 >= r[1] && at.1 < r[1]+r[3] }

impl Menu {
    pub(super) fn new(names: Vec<String>, selected: Option<&str>) -> Self {
        let focus = selected.and_then(|n| names.iter().position(|p| p == n)).unwrap_or(0);
        Self { names, error: None, focus, start: 0 }
    }
    pub(super) fn reveal(&mut self, w: f32, h: f32) {
        let count = capacity(frame(w, h, self.names.len()));
        self.start = self.start.min(self.focus).max((self.focus + 1).saturating_sub(count));
    }
    pub(super) fn scroll(&mut self, steps: i32, w: f32, h: f32) {
        let max = self.names.len().saturating_sub(capacity(frame(w, h, self.names.len())));
        self.start = (self.start as i32 + steps).clamp(0, max as i32) as usize;
    }
    pub(super) fn key(&mut self, key: &str, w: f32, h: f32) -> Option<String> {
        match key {
            "key.keyboard.up" => self.focus = self.focus.saturating_sub(1),
            "key.keyboard.down" => self.focus = (self.focus + 1).min(self.names.len().saturating_sub(1)),
            "key.keyboard.enter" | "key.keyboard.numpad.enter" => return self.names.get(self.focus).cloned(),
            _ => return None,
        }
        self.reveal(w, h);
        None
    }
    // None outside the list; clicking elsewhere is consumed by the modal.
    pub(super) fn click(&self, at: (f32, f32), w: f32, h: f32) -> Option<String> {
        let r = frame(w, h, self.names.len());
        let list = [r[0]+4.0, r[1]+25.0, r[2]-8.0, capacity(r) as f32 * ROW];
        if !inside(list, at) { return None; }
        self.names.get(self.start + ((at.1-list[1])/ROW) as usize).cloned()
    }
    pub(super) fn closes(&self, at: (f32, f32), w: f32, h: f32) -> bool {
        let r = frame(w, h, self.names.len());
        !inside(r, at) || inside([r[0]+r[2]-25.0,r[1],25.0,22.0], at)
    }
}

fn fit(game: &Game<'_>, text: &str, width: f32) -> String {
    if game.native_text_width(text) <= width { return text.into(); }
    let mut s = text.to_owned();
    while !s.is_empty() && game.native_text_width(&format!("{s}...")) > width { s.pop(); }
    format!("{s}...")
}
pub(super) fn draw(game: &Game<'_>, w: f32, h: f32, at: (f32,f32), label: &str, selected: Option<&str>, menu: Option<&Menu>, out: &mut Vec<Shape>) {
    let b = button(w);
    out.push(Shape::Round(b, 3.0, if inside(b,at) { 0xF830_3030 } else { 0xF818_1818 }));
    out.push(Shape::Text(fit(game,&format!("Profile: {label}"),b[2]-17.0),[b[0]+4.0,b[1]+3.0],accent()));
    out.push(Shape::Text("v".into(),[b[0]+b[2]-9.0,b[1]+3.0],0xFFFFFF));
    let Some(menu) = menu else { return; };
    let r = frame(w,h,menu.names.len());
    out.push(Shape::Fill([0.0,0.0,w,h],0x8000_0000));
    out.push(Shape::Round(r,5.0,0xFF16_1616));
    out.push(Shape::Head("Choose profile".into(),[r[0]+8.0,r[1]+7.0],0xFFFFFF));
    out.push(Shape::Text("x".into(),[r[0]+r[2]-15.0,r[1]+7.0],0xFFFFFF));
    if menu.names.is_empty() {
        out.push(Shape::Text("No saved profiles".into(),[r[0]+8.0,r[1]+28.0],0xC0C0C0));
    }
    for (i,name) in menu.names.iter().enumerate().skip(menu.start).take(capacity(r)) {
        let row = [r[0]+4.0,r[1]+25.0+(i-menu.start) as f32*ROW,r[2]-8.0,ROW];
        let chosen = selected == Some(name.as_str());
        if chosen || inside(row,at) || i == menu.focus {
            out.push(Shape::Round(row,3.0,if chosen { 0xFF38_3838 } else { 0xFF28_2828 }));
        }
        let tag = if chosen { "Selected" } else { "" };
        let tag_w = game.native_text_width(tag);
        out.push(Shape::Text(fit(game,name,row[2]-20.0-tag_w),[row[0]+5.0,row[1]+5.0],if chosen { accent() } else { 0xFFFFFF }));
        if chosen { out.push(Shape::Text(tag.into(),[row[0]+row[2]-tag_w-5.0,row[1]+5.0],0xFFFFFF)); }
    }
    let hint = menu.error.as_deref().unwrap_or("Click to load / arrows + Enter / Esc to close");
    out.push(Shape::Text(fit(game,hint,r[2]-16.0),[r[0]+8.0,r[1]+r[3]-23.0],if menu.error.is_some() { 0xFF9090 } else { 0xC0C0C0 }));
    if menu.names.len()>capacity(r) {
        let text=format!("{}-{} / {}",menu.start+1,(menu.start+capacity(r)).min(menu.names.len()),menu.names.len());
        out.push(Shape::Text(text,[r[0]+8.0,r[1]+r[3]-12.0],0xC0C0C0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_and_keyboard_stay_within_saved_rows() {
        let mut m=Menu::new((0..128).map(|i| format!("profile-{i}")).collect(),None);
        m.scroll(1000,364.0,240.0);
        assert_eq!(m.start,119);
        m.key("key.keyboard.up",364.0,240.0);
        assert_eq!(m.start,0);
        for _ in 0..200 { m.key("key.keyboard.down",364.0,240.0); }
        assert_eq!(m.key("key.keyboard.enter",364.0,240.0).as_deref(),Some("profile-127"));
    }
}
