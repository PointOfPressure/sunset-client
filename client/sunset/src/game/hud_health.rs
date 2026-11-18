// HUD's Low health warning (off by default): below Low health below (health points; 2 to a heart, absorption not
// counted) the screen's edges take a red tint that deepens as your health goes down (Edge tint), or pulse, faster the
// lower it is (Flash). At Low health strength (0.1 to 1) at its most. Nothing in spectator mode, while dead or with
// no player; F1 hides it with the rest of the HUD.
use super::Game;
use super::sunset::{Module, Shape};
use std::sync::Mutex;
use std::time::Instant;

// The tint's colour, and how many bands each edge's fade is drawn in.
const RED: u32 = 0x00C8_1010;
const BANDS: usize = 8;

static START: Mutex<Option<Instant>> = Mutex::new(None);

fn seconds() -> f32 {
    START.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Instant::now).elapsed().as_secs_f32()
}

// How strong the warning is, 0 (none) to 1: none at or above `below`, then from a third at the threshold to all of it
// at no health left, so crossing the line is already visible.
pub(super) fn level(health: f32, below: f32) -> f32 {
    if !(health.is_finite() && below > 0.0) || health <= 0.0 || health >= below {
        return 0.0;
    }
    let low = 1.0 - health / below;
    (1.0 / 3.0 + low * 2.0 / 3.0).clamp(0.0, 1.0)
}

// Flash: 0 to 1 and back, about once a second at the threshold and three times a second at the end.
pub(super) fn pulse(level: f32, t: f32) -> f32 {
    let hz = 1.0 + 2.0 * level.clamp(0.0, 1.0);
    0.5 - 0.5 * (t * hz * std::f32::consts::TAU).cos()
}

// The edge fade's bands, from the screen's edge inwards: each band's alpha (0 to 255) at `strength` (0 to 1).
pub(super) fn band_alphas(strength: f32) -> [u32; BANDS] {
    let mut out = [0; BANDS];
    for (i, a) in out.iter_mut().enumerate() {
        let fall = 1.0 - i as f32 / BANDS as f32;
        *a = ((strength.clamp(0.0, 1.0) * fall * fall * 200.0).round() as u32).min(255);
    }
    out
}

pub(super) fn draw(game: &Game<'_>, hud: &Module, w: f32, h: f32, out: &mut Vec<Shape>) {
    if !hud.b("Low health warning") || game.spectator() {
        return;
    }
    let Some(health) = game.player.as_ref().map(|p| p.health.health()) else {
        return;
    };
    let level = level(health, hud.n("Low health below") as f32);
    if level <= 0.0 {
        return;
    }
    let mut strength = level * hud.n("Low health strength") as f32;
    if hud.p("Low health style") == "Flash" {
        strength *= pulse(level, seconds());
    }
    // Each edge a twelfth of the screen's shorter side deep, in bands that fade inwards; the corners overlap, a
    // little deeper there.
    let depth = (w.min(h) / 12.0).max(8.0);
    let step = depth / BANDS as f32;
    for (i, alpha) in band_alphas(strength).into_iter().enumerate() {
        if alpha == 0 {
            continue;
        }
        let argb = alpha << 24 | RED;
        let d = i as f32 * step;
        out.push(Shape::Fill([0.0, d, w, step], argb));
        out.push(Shape::Fill([0.0, h - d - step, w, step], argb));
        out.push(Shape::Fill([d, 0.0, step, h], argb));
        out.push(Shape::Fill([w - d - step, 0.0, step, h], argb));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_warning_starts_below_the_threshold_and_deepens() {
        assert_eq!(level(20.0, 6.0), 0.0);
        assert_eq!(level(6.0, 6.0), 0.0);
        assert!((level(5.999, 6.0) - 1.0 / 3.0).abs() < 1e-3);
        assert!(level(3.0, 6.0) > level(5.0, 6.0));
        assert!((level(0.001, 6.0) - 1.0).abs() < 1e-3);
        // Dead, no threshold, or nonsense: nothing.
        assert_eq!(level(0.0, 6.0), 0.0);
        assert_eq!(level(3.0, 0.0), 0.0);
        assert_eq!(level(f32::NAN, 6.0), 0.0);
    }

    #[test]
    fn the_flash_pulses_faster_when_lower() {
        assert_eq!(pulse(0.5, 0.0), 0.0);
        // At the threshold one flash a second: full at half a second, back to none at a second.
        assert!((pulse(0.0, 0.5) - 1.0).abs() < 1e-4 && pulse(0.0, 1.0) < 1e-4);
        // At the end three a second.
        assert!((pulse(1.0, 1.0 / 6.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn the_edge_fades_inwards_and_vanishes_at_zero() {
        let a = band_alphas(1.0);
        assert_eq!(a[0], 200);
        assert!(a.windows(2).all(|p| p[0] > p[1]));
        assert_eq!(band_alphas(0.0), [0; BANDS]);
        assert_eq!(band_alphas(7.0), a);
    }
}
