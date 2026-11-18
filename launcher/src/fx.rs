//! The launcher's motion. Controls light up on hover and dim while pressed, the tab pill slides, menus fade in and
//! out. Everything runs on critically damped springs or one-shot animations, so nothing overshoots and nothing
//! redraws once it has settled: an idle window draws no frames.

use gpui::{
    AnimationExt as _, Div, ElementId, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, SpringAnimation, SpringConfig,
    Rgba, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb, rgba,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

/// Hover fades: a damping ratio just over 1 (no overshoot), most of the way there in about 120 ms.
pub const HOVER: SpringConfig = SpringConfig::new(1400., 76., 1.);
/// Press feedback: quicker, so a click reads at once.
const PRESS: SpringConfig = SpringConfig::new(3600., 122., 1.);
/// The tab pill: an ease-out slide of about 180 ms.
pub const SLIDE: SpringConfig = SpringConfig::new(900., 62., 1.);
/// Opacity closer than this to its target is drawn as the target, and the spring stops asking for frames.
const SETTLED: f32 = 0.004;
/// Menus fade and slide in over this, and out over `MENU_OUT`.
pub const MENU_IN: Duration = Duration::from_millis(150);
pub const MENU_OUT: Duration = Duration::from_millis(120);
/// The dim a pressed control gets on top of its hover colour.
const PRESS_DIM: u32 = 0x0000002e;

#[derive(Clone, Copy, Default)]
struct Hot {
    hover: bool,
    press: bool,
    seen: u64,
}

thread_local! {
    // Hover and press per control id. The UI runs on one thread, and the free `button`-style builders have no
    // context to hang state on, so it lives here.
    static HOT: RefCell<HashMap<ElementId, Hot>> = RefCell::new(HashMap::new());
    static FRAME: Cell<u64> = const { Cell::new(0) };
}

/// Call once at the top of each render. A control left out of a render (a closed menu's row, the Play button while
/// a job runs) never hears the pointer leave, so when it comes back it starts unlit instead of stuck lit.
pub fn begin_frame() {
    FRAME.with(|f| f.set(f.get() + 1));
}

fn hot(id: &ElementId) -> Hot {
    let frame = FRAME.with(Cell::get);
    HOT.with_borrow_mut(|m| {
        let h = m.entry(id.clone()).or_default();
        if h.seen + 1 < frame {
            h.hover = false;
            h.press = false;
        }
        h.seen = frame;
        *h
    })
}

fn set(id: &ElementId, window: &mut Window, f: impl FnOnce(&mut Hot)) {
    HOT.with_borrow_mut(|m| f(m.entry(id.clone()).or_default()));
    window.refresh();
}

/// A layer filling its parent (inside any border) whose opacity springs towards `on`.
fn layer(id: &'static str, colour: gpui::Rgba, radius: f32, config: SpringConfig, on: bool) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .rounded(px(radius))
        .bg(colour)
        .with_spring(id, SpringAnimation::new(config).to(if on { 1f32 } else { 0. }).with_epsilon(SETTLED), |d, t| d.opacity(t))
}

/// A clickable control with id `id` that fades to `light` under the pointer and dims a little while pressed. It
/// replaces `div().id(id)` plus a `.hover(..)` background; `radius` is the corner radius inside its border. Add
/// children after this: the light and the dim are layers under them.
pub fn control(id: impl Into<ElementId>, light: u32, radius: f32) -> Stateful<Div> {
    pressable(id.into(), Some(light), radius)
}

/// Whether the pointer is over the control `id`, for controls that show hover some other way than a light.
pub fn hovered(id: &ElementId) -> bool {
    hot(id).hover
}

/// `a` blended towards `b` by `t`.
pub fn mix(a: u32, b: u32, t: f32) -> Rgba {
    let (a, b) = (rgb(a), rgb(b));
    Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
}

/// `control`, with the hover light optional.
pub fn pressable(id: ElementId, light: Option<u32>, radius: f32) -> Stateful<Div> {
    let h = hot(&id);
    let (on_hover, on_down, on_up, on_up_out) = (id.clone(), id.clone(), id.clone(), id.clone());
    div()
        .id(id)
        .relative()
        .children(light.map(|l| layer("fx-hover", rgb(l), radius, HOVER, h.hover)))
        .child(layer("fx-press", rgba(PRESS_DIM), radius, PRESS, h.press))
        .on_hover(move |on, window, _| set(&on_hover, window, |h| h.hover = *on))
        .on_mouse_down(MouseButton::Left, move |_, window, _| set(&on_down, window, |h| h.press = true))
        .on_mouse_up(MouseButton::Left, move |_, window, _| set(&on_up, window, |h| h.press = false))
        .on_mouse_up_out(MouseButton::Left, move |_, window, _| set(&on_up_out, window, |h| h.press = false))
}

/// Where the tab pill sits at `at` (a tab index, fractional while it slides), from each tab's left edge and width.
pub fn pill_span(tabs: &[(f32, f32)], at: f32) -> (f32, f32) {
    let Some(last) = tabs.len().checked_sub(1) else { return (0., 0.) };
    let at = at.clamp(0., last as f32);
    let (i, f) = (at.floor() as usize, at.fract());
    let (a, b) = (tabs[i], tabs[(i + 1).min(last)]);
    (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pill_rests_on_a_tab_and_blends_between_two() {
        let tabs = [(0., 100.), (102., 80.), (184., 90.)];
        assert_eq!(pill_span(&tabs, 0.), (0., 100.));
        assert_eq!(pill_span(&tabs, 2.), (184., 90.));
        assert_eq!(pill_span(&tabs, 0.5), (51., 90.));
        // Past either end (a spring still settling, or a tab list that just shrank) it stays on the end tabs.
        assert_eq!(pill_span(&tabs, -0.2), (0., 100.));
        assert_eq!(pill_span(&tabs, 7.), (184., 90.));
        assert_eq!(pill_span(&[], 1.), (0., 0.));
    }

    #[test]
    fn a_control_missing_from_a_render_comes_back_unlit() {
        let id = ElementId::from("t-ctl");
        begin_frame();
        hot(&id);
        HOT.with_borrow_mut(|m| m.get_mut(&id).unwrap().hover = true);
        begin_frame();
        assert!(hot(&id).hover, "still lit while it stays on screen");
        begin_frame();
        begin_frame();
        assert!(!hot(&id).hover && !hot(&id).press, "skipped a render, so unlit");
    }

    #[test]
    fn springs_do_not_overshoot() {
        for c in [HOVER, PRESS, SLIDE] {
            assert!(c.canonical().1 >= 1.0, "damping ratio below 1 would bounce");
        }
    }
}
