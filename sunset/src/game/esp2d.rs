// ESP's 2D box extras: the box's style (a full box, its corners, or side brackets) with a dark edge under it, a glow
// round it, a health bar on one side (with the health as a number), the name over it and the distance under it;
// and Shield colors, a player's box coloured by their shield (held or raised, or put out by an axe in the last five
// seconds).
use super::Game;
use super::sunset::{Camera, Canvas, Module, outline};
use super::sunset_effects::{alpha, mix};
use horizon_client::mobs::Mob;

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Box,
    Corners,
    Brackets,
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
    Bottom,
    Top,
}

#[derive(Clone, Copy, PartialEq)]
enum ShieldMode {
    Always,
    Raised,
    Broken,
}

// ESP's 2D settings, read once a frame.
pub(super) struct TwoD {
    style: Style,
    fill: Option<f64>,
    health: Option<Side>,
    health_text: bool,
    names: bool,
    distance: bool,
    glow: bool,
    shield: Option<(ShieldMode, u32, u32)>,
}

impl TwoD {
    pub(super) fn of(m: &Module) -> Self {
        Self {
            style: match m.p("2D style") {
                "Corners" => Style::Corners,
                "Brackets" => Style::Brackets,
                _ => Style::Box,
            },
            fill: (m.p("Shape mode") != "Lines").then(|| m.n("Fill opacity")),
            health: m.b("Health bar").then(|| match m.p("Health side") {
                "Right" => Side::Right,
                "Bottom" => Side::Bottom,
                "Top" => Side::Top,
                _ => Side::Left,
            }),
            health_text: m.b("Health text"),
            names: m.b("2D names"),
            distance: m.b("2D distance"),
            glow: m.b("Glow"),
            shield: m.b("Shield colors").then(|| {
                let mode = match m.p("Shield mode") {
                    "Raised" => ShieldMode::Raised,
                    "Broken" => ShieldMode::Broken,
                    _ => ShieldMode::Always,
                };
                (mode, m.c("Shield color"), m.c("Broken shield color"))
            }),
        }
    }
    // Whether a 2D box needs more than the plain outline.
    pub(super) fn extras(&self) -> bool {
        self.style != Style::Box || self.health.is_some() || self.names || self.distance || self.glow
    }
}

// Shield colors: the box's colour for a player by their shield, if it says one.
pub(super) fn shield_colour(game: &Game<'_>, two: &TwoD, mob: &Mob) -> Option<u32> {
    let (mode, up, broken) = two.shield?;
    if mob.kind.id != "minecraft:player" {
        return None;
    }
    let has = |item: &Option<std::sync::Arc<str>>| item.as_deref() == Some("minecraft:shield");
    let holds = has(&mob.extra.main_hand) || has(&mob.extra.off_hand);
    let raised = mob.extra.blocking && mob.extra.using_item > 0;
    let out = super::sunset_effects::shield_broken(game, mob.id);
    match mode {
        _ if out && mode != ShieldMode::Raised => Some(broken),
        ShieldMode::Always if holds => Some(up),
        ShieldMode::Raised if raised => Some(up),
        _ => None,
    }
}

fn bar(out: &mut impl Canvas, r: [f32; 4], argb: u32) {
    if r[2] > 0.0 && r[3] > 0.0 {
        out.fill(r, argb);
    }
}

// The box round a mob as the 2D settings say. True when it was on screen.
pub(super) fn draw(game: &Game<'_>, cam: &Camera, out: &mut impl Canvas, two: &TwoD, mob: &Mob, lo: [f64; 3], hi: [f64; 3], d: f64, argb: u32, lw: f32) -> bool {
    let Some([x1, y1, x2, y2]) = cam.corners(lo, hi).and_then(|p| cam.bounds(&p)) else {
        return false;
    };
    let (w, h) = (x2 - x1, y2 - y1);
    if w < 3.0 || h < 3.0 {
        return false;
    }
    let a = (argb >> 24) as f32 / 255.0;
    let shadow = alpha(0, a * 0.47);
    if two.glow {
        for i in 1..=4 {
            let grow = 2.0 * i as f32;
            outline(out, [x1 - grow, y1 - grow, x2 + grow, y2 + grow], alpha(argb, a * 0.14 * (1.0 - i as f32 / 5.0)), lw, false);
        }
    }
    if let Some(fill) = two.fill {
        out.fill([x1, y1, w, h], alpha(argb, a * fill as f32));
    }
    let t = lw.max(1.0);
    match two.style {
        Style::Box => {
            outline(out, [x1 - 1.0, y1 - 1.0, x2 + 1.0, y2 + 1.0], shadow, t, false);
            outline(out, [x1, y1, x2, y2], argb, t, false);
        }
        Style::Corners => {
            let len = (w.min(h) * 0.28).max(2.0);
            for (colour, grow) in [(shadow, 1.0), (argb, 0.0)] {
                let (l, r, top, bottom) = (x1 - grow, x2 + grow, y1 - grow, y2 + grow);
                for (x, dx) in [(l, 1.0), (r, -1.0)] {
                    for (y, dy) in [(top, 1.0), (bottom, -1.0)] {
                        let across = if dx > 0.0 { [x, y - t / 2.0, len, t] } else { [x - len, y - t / 2.0, len, t] };
                        let down = if dy > 0.0 { [x - t / 2.0, y, t, len] } else { [x - t / 2.0, y - len, t, len] };
                        bar(out, across, colour);
                        bar(out, down, colour);
                    }
                }
            }
        }
        Style::Brackets => {
            let len = (h * 0.35).max(2.0);
            let mid = (y1 + y2) / 2.0;
            for (colour, grow) in [(shadow, 1.0), (argb, 0.0)] {
                for x in [x1 - grow, x2 + grow] {
                    bar(out, [x - t / 2.0, y1 - grow, t, len], colour);
                    bar(out, [x - t / 2.0, mid - len * 0.2, t, len * 0.4], colour);
                    bar(out, [x - t / 2.0, y2 + grow - len, t, len], colour);
                }
            }
        }
    }
    if let Some(side) = two.health {
        let frac = (mob.health / mob.kind.max_health.max(1.0)).clamp(0.0, 1.0);
        let colour = 0xFF00_0000 | mix(0xFF4A4A, 0x78F58C, frac);
        let back = alpha(0, a * 0.6);
        let (track, full) = match side {
            Side::Left => ([x1 - 5.0, y1, 2.0, h], [x1 - 5.0, y2 - h * frac, 2.0, h * frac]),
            Side::Right => ([x2 + 3.0, y1, 2.0, h], [x2 + 3.0, y2 - h * frac, 2.0, h * frac]),
            Side::Bottom => ([x1, y2 + 3.0, w, 2.0], [x1, y2 + 3.0, w * frac, 2.0]),
            Side::Top => ([x1, y1 - 5.0, w, 2.0], [x1, y1 - 5.0, w * frac, 2.0]),
        };
        bar(out, [track[0] - 0.5, track[1] - 0.5, track[2] + 1.0, track[3] + 1.0], back);
        bar(out, full, colour);
        if two.health_text {
            let hp = (mob.health.max(0.0).round() as i32).to_string();
            let tw = game.native_text_width(&hp);
            let at = match side {
                Side::Left => [track[0] - tw - 2.0, full[1] - 4.0],
                Side::Right => [track[0] + 4.0, full[1] - 4.0],
                Side::Bottom => [x1 + w * frac - tw / 2.0, track[1] + 3.0],
                Side::Top => [x1 + w * frac - tw / 2.0, track[1] - 11.0],
            };
            out.text(&hp, at, 0xF0F0F5);
        }
    }
    if two.names {
        let mut name = String::new();
        super::sunset_util::name_into(game, mob, &mut name);
        let tw = game.native_text_width(&name);
        let at = [(x1 + x2) / 2.0 - tw / 2.0, y1 - 14.0];
        out.fill([at[0] - 3.0, at[1] - 2.0, tw + 6.0, 12.0], alpha(0x10_1016, a * 0.6));
        out.text(&name, at, 0xF0F0FA);
    }
    if two.distance {
        let text = format!("{}m", d.round() as i64);
        let tw = game.native_text_width(&text);
        let at = [(x1 + x2) / 2.0 - tw / 2.0, y2 + 4.0];
        out.fill([at[0] - 2.0, at[1] - 1.0, tw + 4.0, 11.0], alpha(0x10_1016, a * 0.5));
        out.text(&text, at, 0xC8C8DC);
    }
    true
}
