// How things are drawn, the way Meteor's ESP, Tracers and Nametags let you choose: a colour palette, 3D or 2D
// boxes, lines, sides or both, a fill opacity and a fade near you; and the cheap ways every drawing module offers
// (an outline, Potato).

use super::sunset::{Camera, Canvas, Kind, Module, Setting, Value, line, outline};
use super::sunset_util::dist;

pub(super) const PALETTE: &[&str] = &[
    "White", "Gold", "Red", "Orange", "Yellow", "Lime", "Green", "Cyan", "Blue", "Purple", "Pink", "Gray", "Black", "Sunset Orange",
];

pub(super) fn palette(name: &str) -> u32 {
    match name {
        "Gold" => 0xFFFF_B020,
        "Sunset Orange" => 0xFFDB_5000,
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

// Which kinds of entity a module draws and in what colours, read once a frame instead of once a mob.
pub(super) struct Kinds {
    want: [bool; 3],
    colour: [u32; 3],
    friend_colours: bool,
    // Mob types drawn whatever their kind (the Entities list), or left out (List mode Blacklist).
    extra: String,
    blacklist: bool,
}

impl Kinds {
    pub(super) fn of(m: &Module) -> Self {
        Self {
            want: [m.b("Players"), m.b("Hostiles"), m.b("Passive")],
            colour: [m.c("Players color"), m.c("Hostiles color"), m.c("Passive color")],
            friend_colours: m.b("Show friend colors"),
            extra: m.t("Entities").to_owned(),
            blacklist: m.p("List mode") == "Blacklist",
        }
    }
    pub(super) fn wants(&self, kind: Kind, id: &str) -> bool {
        super::sunset_util::kind_wanted(self.want[kind as usize], &self.extra, self.blacklist, id)
    }
    pub(super) fn colour(&self, kind: Kind, friend: bool) -> u32 {
        if friend && self.friend_colours { 0xFF55_FFFF } else { self.colour[kind as usize] }
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

// A box's faces: the corners round each (bit 1 of a corner index is x at its high end, bit 2 y, bit 4 z), and the
// side of the box the camera must be on to see it (axis, high end).
const FACES: [([usize; 4], usize, bool); 6] = [
    ([0, 2, 6, 4], 0, false),
    ([1, 3, 7, 5], 0, true),
    ([0, 1, 5, 4], 1, false),
    ([2, 3, 7, 6], 1, true),
    ([0, 1, 3, 2], 2, false),
    ([4, 5, 7, 6], 2, true),
];

// The ways a module can draw a thing (its Mode): a 3D box, the 2D box round it on screen, that box's outline alone,
// or Potato, one small square on its middle, the least work there is.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Draw {
    Box3d,
    Box2d,
    Outline,
    Potato,
}

// How a module draws its boxes, read once a frame from its Mode and Shape mode (or its Fill switch).
pub(super) struct Look {
    pub(super) draw: Draw,
    sides: bool,
    lines: bool,
    fill: f64,
}

impl Look {
    pub(super) fn of(m: &Module) -> Self {
        let draw = match m.p("Mode") {
            "2D box" => Draw::Box2d,
            "Outline" => Draw::Outline,
            "Potato" => Draw::Potato,
            _ => Draw::Box3d,
        };
        // Without a Shape mode: lines and a light fill, unless a Fill switch is off.
        let (sides, lines, fill) = match m.setting("Shape mode") {
            Some(_) => (m.p("Shape mode") != "Lines", m.p("Shape mode") != "Sides", m.n("Fill opacity")),
            None => (m.setting("Fill").is_none() || m.b("Fill"), true, 40.0 / 255.0),
        };
        Self { draw, sides, lines, fill }
    }
    // Lines and a light fill in 3D: the boxes modules show where they place or break, a throw's landing.
    pub(super) const BOXED: Self = Self { draw: Draw::Box3d, sides: true, lines: true, fill: 40.0 / 255.0 };
}

// Potato: one small square on `at`. True when it was on screen.
pub(super) fn potato(cam: &Camera, out: &mut impl Canvas, at: [f64; 3], argb: u32, lw: f32) -> bool {
    let half = 2.0 * lw;
    match cam.project(at).filter(|p| cam.on_screen(*p, half)) {
        Some([x, y]) => {
            out.fill([x - half, y - half, half * 2.0, half * 2.0], argb);
            true
        }
        None => false,
    }
}

// A box in the world as the module's Look says. True when any of it was on screen (what a Max counts). A 3D box's
// sides are its faces turned to the camera (at most three, none from inside it), its edges the lines; a corner
// behind the camera leaves out the faces and edges it belongs to. The 2D modes need the whole box in front.
pub(super) fn draw_box(cam: &Camera, out: &mut impl Canvas, look: &Look, lo: [f64; 3], hi: [f64; 3], argb: u32, lw: f32) -> bool {
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    if look.draw == Draw::Potato {
        return potato(cam, out, mid, argb, lw);
    }
    if !cam.sees(mid, dist(lo, hi) / 2.0) {
        return false;
    }
    let fill = (((argb >> 24) as f64 * look.fill).round() as u32) << 24 | (argb & 0x00FF_FFFF);
    if look.draw == Draw::Box3d {
        let pts = cam.points(lo, hi);
        let eye = cam.eye();
        let mut drawn = false;
        for (face, axis, high) in FACES.iter().filter(|_| look.sides) {
            let facing = if *high { eye[*axis] > hi[*axis] } else { eye[*axis] < lo[*axis] };
            if let (true, [Some(a), Some(b), Some(c), Some(d)]) = (facing, face.map(|i| pts[i])) {
                out.quad([a, b, c, d], fill);
                drawn = true;
            }
        }
        for (a, b) in EDGES.iter().filter(|_| look.lines) {
            if let (Some(p), Some(q)) = (pts[*a], pts[*b]) {
                line(out, p, q, argb, lw);
                drawn = true;
            }
        }
        return drawn;
    }
    let Some(pts) = cam.corners(lo, hi) else {
        return false;
    };
    let Some(r) = cam.bounds(&pts) else {
        return false;
    };
    if look.sides && look.draw != Draw::Outline {
        out.fill([r[0], r[1], r[2] - r[0], r[3] - r[1]], fill);
    }
    if look.lines || look.draw == Draw::Outline {
        outline(out, r, argb, lw, false);
    }
    true
}
