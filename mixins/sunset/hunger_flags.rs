// The game's own bars, as HUD's Mount HUD and food settings ask (set by Sunset each tick): the food row also while
// riding, the mount's hearts moved up over it, the XP bar instead of a jump bar that isn't charging; the held
// food's hunger and saturation drawn on the row, the saturation you have, and the exhaustion under it.
use super::{Blend, GuiDraw, HealthHud, HealthInputs, experience as experience_layout};
use std::borrow::Cow;
use std::cell::Cell;
use std::ops::ControlFlow;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bars {
    // Mount HUD on: the food row and the XP bar while riding.
    pub mount: bool,
    // Riding with Mount HUD on, no jump charging and no waypoints: the XP bar over the jump bar.
    pub mount_xp: bool,
    // How far the mount's hearts move up: the food row, and the air bubbles when they show.
    pub raise: i32,
    // Food and saturation once the held food is eaten.
    pub preview: Option<(i32, f32)>,
    pub saturation: bool,
    // Exhaustion, from 0 to 4 (a point of saturation or food goes at 4).
    pub exhaustion: Option<f32>,
}

pub static BARS: Mutex<Bars> = Mutex::new(Bars { mount: false, mount_xp: false, raise: 0, preview: None, saturation: false, exhaustion: None });

thread_local! {
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

pub fn bars() -> Bars {
    *BARS.lock().unwrap_or_else(|e| e.into_inner())
}

// The food icons' spots: from the right end of the row, (x, y) of the n-th.
fn icon(w: i32, h: i32, n: i32) -> (i32, i32) {
    (w / 2 + 91 - n * 8 - 9, h - 39)
}

fn sprite(name: &'static str, (x, y): (i32, i32), color: u32) -> GuiDraw {
    GuiDraw::Sprite { sprite: Cow::Borrowed(name), x, y, w: 9, h: 9, region: None, color, blend: Blend::Textured }
}

fn fill(x0: i32, y0: i32, x1: i32, y1: i32, color: u32) -> GuiDraw {
    GuiDraw::Fill { x0, y0, x1, y1, color }
}

// A gold frame round the n-th icon for `points` (0 to 2) of saturation: a whole frame for two, the right half for one.
fn frame(out: &mut Vec<GuiDraw>, (x, y): (i32, i32), points: f32, color: u32) {
    if points <= 0.0 {
        return;
    }
    let left = if points >= 2.0 { x } else { x + 9 - (9.0 * points / 2.0).ceil() as i32 };
    out.push(fill(left, y, x + 9, y + 1, color));
    out.push(fill(left, y + 8, x + 9, y + 9, color));
    out.push(fill(x + 8, y + 1, x + 9, y + 8, color));
    if points >= 2.0 {
        out.push(fill(x, y + 1, x + 1, y + 8, color));
    }
}

// HealthHud::layout: the game's own layout, with the food row while riding and the food overlays.
pub fn layout(hud: &mut HealthHud, i: &HealthInputs, w: i32, h: i32) -> ControlFlow<Vec<GuiDraw>> {
    if INSIDE.with(Cell::get) {
        return ControlFlow::Continue(());
    }
    let b = bars();
    let riding = b.mount && i.vehicle_hearts > 0;
    let food_row = i.vehicle_hearts == 0 || riding;
    let overlays = food_row && (b.preview.is_some() || b.saturation || b.exhaustion.is_some());
    if !riding && !overlays {
        return ControlFlow::Continue(());
    }
    let own = HealthInputs { vehicle_hearts: if riding { 0 } else { i.vehicle_hearts }, ..*i };
    let mut out = Vec::new();
    // Exhaustion under the icons, from the right end.
    if let Some(e) = b.exhaustion.filter(|_| food_row) {
        let width = (81.0 * (e / 4.0).clamp(0.0, 1.0)).round() as i32;
        let (right, y) = (w / 2 + 91, h - 39);
        if width > 0 {
            out.push(fill(right - width, y, right, y + 9, 0x60FF_FFFF));
        }
    }
    INSIDE.with(|c| c.set(true));
    // The game lays the air bubbles out here too, over the food row it draws for `own`; the mount's hearts go up over
    // both (`raise`).
    out.extend(hud.layout(&own, w, h));
    INSIDE.with(|c| c.set(false));
    if !food_row {
        return ControlFlow::Break(out);
    }
    // Flashing, as the game's own blink: half a second each way.
    let pulse = ((i.millis % 1000) as f32 / 1000.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    let alpha = |a: f32| (((0.25 + 0.6 * a) * 255.0) as u32) << 24;
    if b.saturation {
        for n in 0..10 {
            frame(&mut out, icon(w, h, n), (i.saturation - 2.0 * n as f32).min(2.0), 0xFFFF_C800);
        }
    }
    if let Some((food, saturation)) = b.preview {
        let full = if i.hunger { "hud/food_full_hunger" } else { "hud/food_full" };
        let half = if i.hunger { "hud/food_half_hunger" } else { "hud/food_half" };
        for n in 0..10 {
            let point = n * 2 + 1;
            if point >= i.food && point < food {
                out.push(sprite(full, icon(w, h, n), alpha(pulse) | 0xFF_FFFF));
            } else if point == food && point > i.food {
                out.push(sprite(half, icon(w, h, n), alpha(pulse) | 0xFF_FFFF));
            }
            if b.saturation {
                let now = (i.saturation - 2.0 * n as f32).clamp(0.0, 2.0);
                let then = (saturation - 2.0 * n as f32).min(2.0);
                if then > now {
                    frame(&mut out, icon(w, h, n), then, alpha(pulse) | 0xFF_C800);
                }
            }
        }
    }
    ControlFlow::Break(out)
}

// vehicle_health: the mount's hearts moved up over the food row.
pub fn raise(mut draws: Vec<GuiDraw>) -> Vec<GuiDraw> {
    let b = bars();
    if b.mount && b.raise != 0 {
        for d in &mut draws {
            if let GuiDraw::Sprite { y, .. } = d {
                *y -= b.raise;
            }
        }
    }
    draws
}

// experience: with Mount HUD's XP bar, the bar the game leaves out for the jump bar.
pub fn experience(w: i32, h: i32, progress: f32, level: i32, bar: bool, width: &dyn Fn(&str) -> i32) -> ControlFlow<Vec<GuiDraw>> {
    if bar || INSIDE.with(Cell::get) || !bars().mount_xp {
        return ControlFlow::Continue(());
    }
    INSIDE.with(|c| c.set(true));
    let draws = experience_layout(w, h, progress, level, true, width);
    INSIDE.with(|c| c.set(false));
    ControlFlow::Break(draws)
}
