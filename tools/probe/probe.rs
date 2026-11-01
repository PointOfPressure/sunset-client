// One line a frame: unix time, game tick, yaw, pitch, the partial tick, the eyes where the frame draws them,
// then for each mob within 24 blocks its id and the centre and half sizes of its box where the frame draws it.

use super::Game;
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static OUT: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

pub(super) fn frame(game: &Game<'_>) {
    let out = OUT.get_or_init(|| {
        let path = std::env::var_os("SUNSET_PROBE")?;
        std::fs::OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new)
    });
    let (Some(out), Some(p)) = (out, game.player.as_ref()) else {
        return;
    };
    let a = game.clock.camera_fraction();
    let lerp = |x: f64, y: f64| x + a * (y - x);
    let was = game.previous_position;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    let mut line = String::new();
    let _ = write!(
        line,
        "{now:.6} {} {} {} {a:.5} {:.5} {:.5} {:.5}",
        game.ticks,
        game.yaw,
        game.pitch,
        lerp(was.x, p.position.x),
        lerp(was.y, p.position.y) + f64::from(p.dimensions.eye_height),
        lerp(was.z, p.position.z),
    );
    for m in &game.mobs {
        let (dx, dz) = (m.position.x - p.position.x, m.position.z - p.position.z);
        if dx * dx + dz * dz > 24.0 * 24.0 {
            continue;
        }
        let b = m.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let off = [lerp(m.previous.x, m.position.x) - m.position.x, lerp(m.previous.y, m.position.y) - m.position.y, lerp(m.previous.z, m.position.z) - m.position.z];
        let _ = write!(
            line,
            " {} {:.5} {:.5} {:.5} {:.4} {:.4}",
            m.id,
            (lo.x + hi.x) / 2.0 + off[0],
            (lo.y + hi.y) / 2.0 + off[1],
            (lo.z + hi.z) / 2.0 + off[2],
            (hi.x - lo.x) / 2.0,
            (hi.y - lo.y) / 2.0,
        );
    }
    line.push('\n');
    // How walking looks in this frame: whether a screen is open, the view bob and walk distance the camera and hand
    // move by, the own model's limb swing speed and position, and each other player's limb swing as seen here.
    let _ = write!(
        line,
        "walk {} {} {:.5} {:.5}",
        game.ticks,
        u8::from(game.screen != super::Screen::Playing),
        game.avatar.interpolated_bob(a as f32),
        game.avatar.backwards_interpolated_walk_distance(a as f32),
    );
    if let Some(m) = game.player_model.as_ref() {
        let _ = write!(line, " {:.5} {:.5} {:.3}", m.limb_speed, m.limb_position, m.body_yaw);
    }
    for m in game.mobs.iter().filter(|m| m.kind.id.ends_with("player")) {
        let _ = write!(line, " p{} {:.5} {:.5}", m.id, m.limb_speed, m.limb_position);
    }
    line.push('\n');
    if let Ok(mut f) = out.lock() {
        let _ = f.write_all(line.as_bytes());
    }
}
