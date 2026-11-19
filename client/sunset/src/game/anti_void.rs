// Anti Void's Blink mode, after the behaviour of Rise's (written anew for Sunset). While you are over the void (no
// block or liquid under you for 50 blocks), your move packets are held back the way Blink holds them, so the server
// keeps you where you last were over ground. Fall past Fall distance and you are put back there, the held moves
// dropped: the server never saw the fall. Come back over ground first and the hold lets go with one position packet
// to where you are, as Blink's does. Bounce, the other mode, is runtime_tick's.
use super::Game;
use super::sunset::Module;
use super::sunset_util::{facts, replaceable};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

// How far down a floor still counts.
const DEPTH: i32 = 50;
// The longest hold (ticks) before it lets go anyway.
const MAX_HOLD: u32 = 100;

static HOLD: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct State {
    // Where you last were over ground (the last position the server was sent before the hold), and the hold's ticks.
    safe: Option<[f64; 3]>,
    held: u32,
}

static STATE: Mutex<State> = Mutex::new(State { safe: None, held: 0 });

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}

// Whether Anti Void holds the move packets back (runtime_packets drops them as Blink's).
pub(super) fn holding() -> bool {
    HOLD.load(Ordering::Relaxed)
}

// Nothing under the player's box for DEPTH blocks: neither a block to stand on nor a liquid to fall into.
fn over_void(game: &Game<'_>, lo: [f64; 3], hi: [f64; 3]) -> bool {
    let top = (lo[1] - 1e-4).floor() as i32;
    let bottom = (top - DEPTH).max(game.world.shape.min_y);
    for x in (lo[0] + 1e-4).floor() as i32..=(hi[0] - 1e-4).floor() as i32 {
        for z in (lo[2] + 1e-4).floor() as i32..=(hi[2] - 1e-4).floor() as i32 {
            let floor = (bottom..=top).rev().any(|y| {
                let p = [x, y, z];
                !replaceable(game, p) || facts(game, p).fluid != crate::game::Fluid::None
            });
            if floor {
                return false;
            }
        }
    }
    true
}

// Let go: one position packet to where you are now (when this tick's move was held), as Blink's release.
fn release(game: &mut Game<'_>) {
    if HOLD.swap(false, Ordering::Relaxed) {
        super::sunset::flush_blink(game);
    }
    st(|s| s.held = 0);
}

// Off or another mode: nothing held, nothing kept.
pub(super) fn off(game: &mut Game<'_>) {
    release(game);
    st(|s| s.safe = None);
}

// Once a tick, after the tick's move packet.
pub(super) fn tick(game: &mut Game<'_>, m: &Module, blink_on: bool, flight_on: bool) {
    if m.p("Mode") != "Blink" {
        off(game);
        return;
    }
    let Some(p) = game.player.as_ref() else {
        off(game);
        return;
    };
    // Not while you can't fall, glide, ride or fly, nor while Blink itself holds the moves.
    if game.creative || game.spectator() || p.flying || p.fall_flying || game.riding.is_some() || blink_on || flight_on {
        off(game);
        return;
    }
    let pos = [p.position.x, p.position.y, p.position.z];
    let (lo, hi) = ([pos[0] - 0.3, pos[1], pos[2] - 0.3], [pos[0] + 0.3, pos[1] + 1.8, pos[2] + 0.3]);
    if p.ground.on_ground || !over_void(game, lo, hi) {
        // Over ground: what was held goes as one move to here, and here is the place to come back to.
        release(game);
        st(|s| s.safe = Some(pos));
        return;
    }
    let Some(safe) = st(|s| s.safe) else {
        return;
    };
    let fallen = p.fall_distance;
    if f64::from(fallen) > m.n("Fall distance") {
        // Back to where the server still has you; the held moves never go.
        if let Some(p) = game.player.as_mut() {
            p.position.x = safe[0];
            p.position.y = safe[1];
            p.position.z = safe[2];
            p.velocity = p.velocity.scale(0.0);
            p.fall_distance = 0.0;
        }
        HOLD.store(false, Ordering::Relaxed);
        super::sunset::BLINK_HELD.store(false, Ordering::Relaxed);
        st(|s| s.held = 0);
        return;
    }
    HOLD.store(true, Ordering::Relaxed);
    let held = st(|s| {
        s.held += 1;
        s.held
    });
    if held >= MAX_HOLD {
        release(game);
        st(|s| s.safe = None);
    }
}
