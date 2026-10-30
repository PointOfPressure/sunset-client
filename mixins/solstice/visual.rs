// How things are drawn, the way Meteor's ESP, Tracers and Nametags let you choose: a colour palette, 3D or 2D
// boxes, lines, sides or both, a fill opacity and a fade near you.

use super::solstice::{Camera, Kind, Module, Setting, Shape, Value, line, outline};

pub(super) const PALETTE: &[&str] = &[
    "White", "Red", "Orange", "Yellow", "Lime", "Green", "Cyan", "Blue", "Purple", "Pink", "Gray", "Black",
];

pub(super) fn palette(name: &str) -> u32 {
    match name {
        "Red" => 0xFFFF_4040,
        "Orange" => 0xFFFF_A030,
        "Yellow" => 0xFFFF_FF50,
        "Lime" => 0xFF90_FF40,
        "Green" => 0xFF50_FF50,
        "Cyan" => 0xFF50_FFFF,
        "Blue" => 0xFF50_80FF,
        "Purple" => 0xFFB0_50FF,
        "Pink" => 0xFFFF_70C0,
        "Gray" => 0xFFA0_A0A0,
        "Black" => 0xFF10_1010,
        _ => 0xFFFF_FFFF,
    }
}

impl Module {
    // A colour setting: a choice from the palette.
    pub(super) fn colour(mut self, name: &'static str, default: &str) -> Self {
        let i = PALETTE.iter().position(|p| *p == default).unwrap_or(0);
        self.settings.push(Setting {
            name,
            value: Value::Pick { i, of: PALETTE },
        });
        self
    }
    // Its colour, as ARGB.
    pub(super) fn c(&self, name: &str) -> u32 {
        palette(self.p(name))
    }
}

pub(super) fn entity_colour(m: &Module, kind: Kind, friend: bool) -> u32 {
    if friend && m.b("Show friend colors") {
        return 0xFF55_FFFF;
    }
    match kind {
        Kind::Player => m.c("Players color"),
        Kind::Hostile => m.c("Hostiles color"),
        Kind::Passive => m.c("Passive color"),
    }
}

// The colour fading in as something comes near, from `fade` blocks in.
pub(super) fn faded(argb: u32, distance: f64, fade: f64) -> u32 {
    if fade <= 0.0 || distance >= fade {
        return argb;
    }
    let a = ((argb >> 24) as f64 * (distance / fade).clamp(0.1, 1.0)).round() as u32;
    (a << 24) | (argb & 0x00FF_FFFF)
}

// Green when far, red when close (Meteor's tracers by distance).
pub(super) fn distance_colour(distance: f64) -> u32 {
    let t = (distance / 60.0).clamp(0.0, 1.0);
    let (r, g) = (((1.0 - t) * 255.0) as u32, (t * 255.0) as u32);
    0xFF00_0040 | (r << 16) | (g << 8)
}

const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

// A box in the world as the module's Mode and Shape mode say.
pub(super) fn draw_box(cam: &Camera, out: &mut Vec<Shape>, m: &Module, lo: [f64; 3], hi: [f64; 3], argb: u32, lw: f32) {
    let Some(r) = cam.rect(lo, hi) else {
        return;
    };
    let shape = m.p("Shape mode");
    if shape != "Lines" {
        let a = ((argb >> 24) as f64 * m.n("Fill opacity")).round() as u32;
        out.push(Shape::Fill([r[0], r[1], r[2] - r[0], r[3] - r[1]], (a << 24) | (argb & 0x00FF_FFFF)));
    }
    if shape == "Sides" {
        return;
    }
    if m.p("Mode") == "2D box" {
        outline(out, r, argb, lw, false);
        return;
    }
    let corner = |i: usize| {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    let pts: Vec<Option<[f32; 2]>> = (0..8).map(|i| cam.project(corner(i))).collect();
    for (a, b) in EDGES {
        if let (Some(p), Some(q)) = (pts[a], pts[b]) {
            line(out, p, q, argb, lw);
        }
    }
}
