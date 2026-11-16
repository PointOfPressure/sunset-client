// Auto Sign and its Sign Aura, written fresh from what the servers do. A server takes a sign's text only from the
// player it opened that sign's editor for (vanilla's allowed editor, Horizon's lan_sign_update), once per opening, for
// the side the player stood on when clicking (isFacingFrontText). So every side is a real click on the sign first; what
// is left to us is the rest: the editor the server opens is never shown (a HEAD hook on remote_open_sign_editor), the
// text goes out without it, and the clicks are made for you.
//
// Grim (behaviour read from its source, no code taken) checks a sign click like a block place: the look of the move
// packet after the click must still hit the block (RotationPlace), the eye within reach of it (FarPlace), the clicked
// face turned to the eye (PositionPlace), one click per tick (MultiPlace) and no identical turn twice
// (DuplicateRotPlace). Nothing checks the editor or the text. Its Anticheat setting keeps to all of that; Horizon's own
// anticheat looks at none of it, so that setting clicks many signs a tick.

use super::Game;
use super::sunset::{Module, hold, last_look, look_at, norm, set_setting, turn, turn_then};
use super::sunset_util::*;
use horizon_client::containers::BlockEntity;
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const WHO: &str = "Auto Sign";
// Vanilla's block interaction range: the farthest a sign is clicked from.
pub(super) const REACH: f64 = 4.5;
// A click the server has not answered with an editor in this many ticks failed; a sign fails twice at most.
const ANSWER: u64 = 20;

pub(super) fn module(m: Module) -> Module {
    m.pick("Text source", &["Settings", "First sign"])
        .text("Front 1", "Sunset")
        .text("Front 2", "")
        .text("Front 3", "")
        .text("Front 4", "")
        .text("Back 1", "")
        .text("Back 2", "")
        .text("Back 3", "")
        .text("Back 4", "")
        .bool("Silent", true)
        .bool("Both sides", true)
        .num("Delay", 2.0, 0.0, 100.0, 1.0)
        .bool("Aura", false)
        .pick("Anticheat", &["Grim", "Horizon"])
        .bool("Only blank", false)
        .num("Range", 4.4, 1.0, REACH, 0.1)
        .num("Clicks per tick", 1.0, 1.0, 16.0, 1.0)
        .num("Cooldown", 1.0, 0.0, 40.0, 1.0)
}

// The settings shown: the lines only with Settings as the source, the aura's own with Aura on.
pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        n if n.starts_with("Front ") || n.starts_with("Back ") => m.p("Text source") == "Settings",
        "Anticheat" | "Only blank" | "Range" | "Clicks per tick" | "Cooldown" => m.b("Aura"),
        _ => true,
    }
}

// What each Anticheat pick sets when picked; the settings stay yours to change after.
const PRESETS: &[(&str, &[(&str, &str)])] = &[
    ("Grim", &[("Clicks per tick", "1"), ("Cooldown", "1"), ("Range", "4.4"), ("Delay", "2")]),
    ("Horizon", &[("Clicks per tick", "8"), ("Cooldown", "0"), ("Range", "4.5"), ("Delay", "0")]),
];

#[derive(Default)]
struct St {
    on: bool,
    silent: bool,
    // The text for each side (front, back), None for a side with nothing to write.
    text: [Option<[String; 4]>; 2],
    // First sign: the first text written by hand.
    first: Option<[String; 4]>,
    // Editors the server opened (pos, front), not yet answered.
    opened: Vec<([i32; 3], bool)>,
    // Texts waiting for their tick.
    pending: VecDeque<(u64, play::SignUpdate)>,
    // Our clicks waiting for their editor: pos -> (front, tick).
    clicked: HashMap<[i32; 3], (bool, u64)>,
    // Sides written or given up on, signs with a side written, failed clicks per side.
    done: HashSet<([i32; 3], bool)>,
    touched: HashSet<[i32; 3]>,
    failed: HashMap<([i32; 3], bool), u8>,
    // Ticks until the next click; the tick a click was asked for (its look on the way); Grim's look to keep a tick.
    wait: u32,
    asked: Option<u64>,
    keep: Option<(f32, f32)>,
    rng: u64,
    preset: String,
}

static ST: Mutex<Option<St>> = Mutex::new(None);
static ON: AtomicBool = AtomicBool::new(false);

fn st<R>(f: impl FnOnce(&mut St) -> R) -> R {
    f(ST.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(St::default))
}

// The sign editor the server opens: kept from the screen (true) when Auto Sign will write that side.
pub(super) fn swallow(pos: [i32; 3], front: bool) -> bool {
    if !ON.load(Ordering::Relaxed) {
        return false;
    }
    st(|s| {
        let take = s.on && s.silent && s.text[usize::from(!front)].is_some();
        if take {
            s.opened.push((pos, front));
        }
        take
    })
}

// What goes out: First sign keeps the first text written by hand.
pub(super) fn outgoing(packet: &S) {
    if let S::SignUpdate(p) = packet
        && ON.load(Ordering::Relaxed)
    {
        st(|s| {
            if s.first.is_none() && p.lines.iter().any(|l| !l.is_empty()) {
                s.first = Some(p.lines.clone());
            }
        });
    }
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m.filter(|_| game.remote.is_some() && game.player.is_some()) else {
        ON.store(false, Ordering::Relaxed);
        st(|s| {
            let preset = std::mem::take(&mut s.preset);
            *s = St { preset, ..St::default() };
        });
        return;
    };
    ON.store(true, Ordering::Relaxed);
    preset(game, m);
    let settings = m.p("Text source") == "Settings";
    let line = |side: &str, i: usize| m.t(&format!("{side} {i}")).chars().take(384).collect::<String>();
    let lines = |side: &str| Some([1, 2, 3, 4].map(|i| line(side, i))).filter(|l| l.iter().any(|t| !t.is_empty()));
    st(|s| {
        s.on = true;
        s.silent = m.b("Silent");
        s.text = if settings { [lines("Front"), lines("Back")] } else { [s.first.clone(), s.first.clone()] };
    });
    // Silent off: the editor opened on the screen, closed again at once.
    if let Some(edit) = game.sign_edit.as_ref()
        && st(|s| s.text[usize::from(!edit.front)].is_some())
        && let Some(edit) = game.sign_edit.take()
    {
        game.capture(true);
        let (x, y, z) = edit.pos;
        st(|s| s.opened.push(([x, y, z], edit.front)));
    }
    answer(game, m);
    let now = game.ticks;
    while let Some(p) = st(|s| s.pending.front().is_some_and(|(due, _)| *due <= now).then(|| s.pending.pop_front()).flatten()) {
        game.send(S::SignUpdate(p.1));
    }
    click(game, m);
}

// Each editor opened: the text for its side, after Delay ticks (some servers refuse a sign written too soon after the
// click); a side that already says it needs nothing.
fn answer(game: &Game<'_>, m: &Module) {
    let now = game.ticks;
    let delay = m.n("Delay").max(0.0) as u64;
    st(|s| {
        for (pos, front) in std::mem::take(&mut s.opened) {
            s.clicked.remove(&pos);
            let Some(lines) = s.text[usize::from(!front)].clone() else {
                continue;
            };
            s.done.insert((pos, front));
            s.touched.insert(pos);
            if side_lines(game, pos, front).is_some_and(|l| *l == lines) {
                continue;
            }
            s.pending.push_back((now + delay, play::SignUpdate { pos, lines, front }));
        }
        // Clicks the server never answered: tried again once, then left.
        let late: Vec<_> = s.clicked.iter().filter(|(_, (_, at))| now >= at + ANSWER).map(|(p, (f, _))| (*p, *f)).collect();
        for (pos, front) in late {
            s.clicked.remove(&pos);
            let n = s.failed.entry((pos, front)).or_insert(0);
            *n += 1;
            if *n >= 2 {
                s.done.insert((pos, front));
            }
        }
    });
}

fn side_lines<'a>(game: &'a Game<'_>, pos: [i32; 3], front: bool) -> Option<&'a [String; 4]> {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Sign(sign)) => Some(if front { &sign.front.lines } else { &sign.back.lines }),
        _ => None,
    }
}

fn preset(game: &Game<'_>, m: &Module) {
    let picked = m.p("Anticheat");
    let before = st(|s| std::mem::replace(&mut s.preset, picked.to_owned()));
    if before.is_empty() || before == picked {
        return;
    }
    if let Some((_, fields)) = PRESETS.iter().find(|p| p.0 == picked) {
        for (name, value) in fields.iter() {
            set_setting(game, &format!("autosign.{}", norm(name)), value);
        }
    }
}

// ---- clicking

fn click(game: &mut Game<'_>, m: &Module) {
    let now = game.ticks;
    let grim = m.p("Anticheat") == "Grim";
    // Grim reads the look of the move packet after a click: the same look again, one mouse step off.
    if let Some((yaw, pitch)) = st(|s| s.keep.take()) {
        let g = gcd(game);
        // Never no step at all: the same look sends no packet.
        let (dy, dp) = st(|s| (f32::from(step(&mut s.rng)).max(0.0) * 2.0 - 1.0, f32::from(step(&mut s.rng))));
        turn(WHO, yaw + g * dy, (pitch + g * dp).clamp(-90.0, 90.0));
        return;
    }
    let ready = st(|s| {
        if s.asked.is_some_and(|at| now < at + 5) {
            return false;
        }
        s.asked = None;
        if s.wait > 0 {
            s.wait -= 1;
            return false;
        }
        true
    });
    let aura = m.b("Aura");
    let both = m.b("Both sides");
    if !ready || (!aura && !both) || !in_world(game) || game.player.as_ref().is_none_or(|p| p.shift_key_down) {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let Some(safe) = safe_slot(game) else {
        return;
    };
    let range = m.n("Range").clamp(1.0, REACH);
    let only_blank = m.b("Only blank");
    let mut found: Vec<(f64, [i32; 3], bool)> = st(|s| {
        let mut out = Vec::new();
        for (&(x, y, z), entity) in &game.world.block_entities {
            let BlockEntity::Sign(sign) = entity else {
                continue;
            };
            let pos = [x, y, z];
            let d = box_distance(eye, pos);
            let touched = s.touched.contains(&pos);
            // Without Both sides a sign gets one side; without Aura only the signs you wrote get their other side.
            if d > range || sign.waxed || s.clicked.contains_key(&pos) || (touched != both && (touched || !aura)) {
                continue;
            }
            let Some(front) = facing_front(game, pos) else {
                continue;
            };
            let Some(lines) = s.text[usize::from(!front)].as_ref() else {
                continue;
            };
            let side = if front { &sign.front } else { &sign.back };
            if s.done.contains(&(pos, front)) || side.lines == *lines || side.click.iter().any(Option::is_some) {
                continue;
            }
            // Only blank is the aura's own: Both sides still writes the other side of a sign you wrote.
            if only_blank && !touched && side.lines.iter().any(|l| !l.trim().is_empty()) {
                continue;
            }
            out.push((d, pos, front));
        }
        out
    });
    if found.is_empty() {
        return;
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.truncate(m.n("Clicks per tick").max(1.0) as usize);
    let clicks: Vec<_> = st(|s| found.iter().map(|&(_, pos, front)| (pos, front, aim(eye, pos, &mut s.rng))).collect());
    let (yaw, pitch) = look_at(eye, clicks[0].2.0);
    let (yaw, pitch) = on_gcd(game, yaw, pitch);
    let cooldown = m.n("Cooldown").max(0.0) as u32;
    st(|s| s.asked = Some(now));
    turn_then(WHO, yaw, pitch, move |game| {
        let before = game.selected;
        if safe != before {
            hold(game, safe);
        }
        for &(pos, front, (_, face, cursor)) in &clicks {
            let sequence = sequence(game);
            game.send(S::UseItemOn(play::UseItemOn { hand: 0, pos, face, cursor, inside: false, world_border: false, sequence }));
            game.swing_arm();
            game.send(S::Punch(Empty));
            let tick = game.ticks;
            st(|s| s.clicked.insert(pos, (front, tick)));
        }
        if safe != before {
            hold(game, before);
        }
        let sent = last_look().unwrap_or((yaw, pitch));
        st(|s| {
            s.asked = None;
            s.wait = if grim { cooldown.max(1) } else { cooldown };
            s.keep = grim.then_some(sent);
        });
    });
}

// The slot to click with: the held one unless it holds what a sign takes as dye, ink or wax; else an empty hotbar slot,
// else any other.
fn safe_slot(game: &Game<'_>) -> Option<usize> {
    let applies = |i: usize| {
        game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| {
            let n = short(&s.item);
            n.ends_with("_dye") || n == "glow_ink_sac" || n == "ink_sac" || n == "honeycomb"
        })
    };
    let empty = |i: usize| game.inventory.get(i).is_none_or(|s| s.is_none());
    if !applies(game.selected) {
        return Some(game.selected);
    }
    (0..9).find(|&i| empty(i)).or_else(|| (0..9).find(|&i| !applies(i)))
}

// `SignBlockEntity.isFacingFrontText` for this player: the side of the sign the server opens. None for no sign.
fn facing_front(game: &Game<'_>, pos: [i32; 3]) -> Option<bool> {
    let p = game.player.as_ref()?;
    let id = game.world.block(pos[0], pos[1], pos[2]);
    let form = game.res.facts().get(id).sign_form()?;
    let state = game.pack.assets.registry.state(id)?;
    let prop = |k: &str| state.properties.get(k).map(String::as_str);
    let rot = if form.wall {
        match prop("facing") {
            Some("west") => 90.0,
            Some("north") => 180.0,
            Some("east") => 270.0,
            _ => 0.0,
        }
    } else {
        prop("rotation").and_then(|r| r.parse::<f64>().ok()).unwrap_or(0.0) * 22.5
    };
    Some(front_of(rot, form.wall && !form.hanging, [p.position.x, p.position.z], pos))
}

// Front when the player is within 90 degrees of the way the sign's text faces; a wall sign's board sits against its
// wall, off the block's centre.
fn front_of(rot: f64, board_on_wall: bool, at: [f64; 2], pos: [i32; 3]) -> bool {
    let (cx, cz) = if board_on_wall {
        match rot as i32 {
            90 => (0.9375, 0.5),
            180 => (0.5, 0.9375),
            270 => (0.0625, 0.5),
            _ => (0.5, 0.0625),
        }
    } else {
        (0.5, 0.5)
    };
    let (dx, dz) = (at[0] - (f64::from(pos[0]) + cx), at[1] - (f64::from(pos[2]) + cz));
    let toward = dz.atan2(dx).to_degrees() - 90.0;
    let diff = (toward - rot).rem_euclid(360.0);
    diff.min(360.0 - diff) <= 90.0
}

// The eye's distance to the sign's block.
fn box_distance(eye: [f64; 3], pos: [i32; 3]) -> f64 {
    let d: f64 = (0..3)
        .map(|k| {
            let lo = f64::from(pos[k]);
            let v = (lo - eye[k]).max(eye[k] - lo - 1.0).max(0.0);
            v * v
        })
        .sum();
    d.sqrt()
}

// Where to look and click: a point inside the block near the eye, a little off each time, on the face turned most to
// the eye (0 down, 1 up, 2 north, 3 south, 4 west, 5 east).
fn aim(eye: [f64; 3], pos: [i32; 3], rng: &mut u64) -> ([f64; 3], i32, [f32; 3]) {
    let mut at = [0.0; 3];
    for k in 0..3 {
        let lo = f64::from(pos[k]);
        at[k] = (eye[k] + 0.08 * f64::from(step(rng))).clamp(lo + 0.15, lo + 0.85);
    }
    let mut best = (1, 0.0_f64, true);
    for (k, faces) in [(1usize, (0, 1)), (2, (2, 3)), (0, (4, 5))] {
        let lo = f64::from(pos[k]);
        let (below, above) = (lo - eye[k], eye[k] - lo - 1.0);
        let (out, high) = if above > below { (above, true) } else { (below, false) };
        if out > best.1 {
            best = (if high { faces.1 } else { faces.0 }, out, high);
        }
    }
    let (face, _, high) = best;
    let axis = match face {
        0 | 1 => 1,
        2 | 3 => 2,
        _ => 0,
    };
    let mut cursor = [0.0_f32; 3];
    for k in 0..3 {
        cursor[k] = if k == axis { f32::from(u8::from(high)) } else { (at[k] - f64::from(pos[k])) as f32 };
    }
    (at, face, cursor)
}

// Vanilla's mouse step for the sensitivity set: every turn a mouse makes is a whole number of them.
fn gcd(game: &Game<'_>) -> f32 {
    let f = f64::from(game.settings.sensitivity) * 0.6 + 0.2;
    (f * f * f * 8.0 * 0.15) as f32
}

// A look turned from the last one the server has by whole mouse steps.
fn on_gcd(game: &Game<'_>, yaw: f32, pitch: f32) -> (f32, f32) {
    let g = gcd(game);
    let (ly, lp) = last_look().unwrap_or((game.yaw, game.pitch));
    let wrap = |d: f32| (d + 180.0).rem_euclid(360.0) - 180.0;
    let snap = |from: f32, d: f32| from + (d / g).round() * g;
    let p = snap(lp, pitch - lp);
    (snap(ly, wrap(yaw - ly)), if p.abs() > 90.0 { p - g * p.signum() } else { p })
}

// -1, 0 or 1 (never the same run for long), seeded from the clock.
fn step(rng: &mut u64) -> i8 {
    if *rng == 0 {
        *rng = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64)
            | 1;
    }
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    (*rng % 3) as i8 - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    // A standing sign facing south (rotation 0): front from the south, back from the north, front just past its side.
    #[test]
    fn the_side_is_where_you_stand() {
        assert!(front_of(0.0, false, [0.5, 3.0], [0, 0, 0]));
        assert!(!front_of(0.0, false, [0.5, -3.0], [0, 0, 0]));
        assert!(front_of(0.0, false, [3.0, 0.6], [0, 0, 0]));
        // A wall sign on a north wall faces south: its board sits at z 0.0625, so z 0.3 is in front of it.
        assert!(front_of(0.0, true, [0.5, 0.3], [0, 0, 0]));
        // Facing east (270): front from the east.
        assert!(front_of(270.0, false, [4.0, 0.5], [0, 0, 0]) && !front_of(270.0, false, [-4.0, 0.5], [0, 0, 0]));
    }

    // The face clicked is turned to the eye, and the cursor lies on it inside the block.
    #[test]
    fn the_click_is_on_the_face_seen() {
        let mut rng = 7;
        for (eye, face) in [([0.5, 1.6, 4.0], 3), ([0.5, 1.6, -3.0], 2), ([4.0, 0.5, 0.5], 5), ([-3.0, 0.4, 0.2], 4), ([0.5, 5.0, 0.6], 1)] {
            let (at, f, c) = aim(eye, [0, 0, 0], &mut rng);
            assert_eq!(f, face, "{eye:?}");
            assert!(at.iter().all(|v| (0.15..=0.85).contains(v)) && c.iter().all(|v| (0.0..=1.0).contains(v)));
        }
        assert!((box_distance([0.5, 0.5, 4.9], [0, 0, 0]) - 3.9).abs() < 1e-9);
    }
}
