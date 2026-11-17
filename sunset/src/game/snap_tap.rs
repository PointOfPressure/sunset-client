// Snap Tap (SOCD "last input wins", as Razer's Snap Tap and Wooting's Rappy Snappy): of two opposite move keys held at
// once, the one pressed last moves you, and letting go of it hands back to the other while that is still held. The
// key that loses is out of the game's held keys for the tick only, so the movement and the server's input packet
// agree; it goes back after the tick. Neutral: both cancel out, as without it.
use super::Game;
use super::sunset::{Category, Module};
use std::sync::Mutex;
use winit::keyboard::KeyCode;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Snap Tap", Category::Movement, "Of two opposite move keys, the last pressed wins.")
            .pick("Mode", &["Last input", "Neutral"])
            .bool("Forward and back", true),
    ]
}

const KEYS: [KeyCode; 4] = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD];

#[derive(Default)]
struct Snap {
    // Which of W, S, A and D were held last tick, and when each was last pressed (a count of presses).
    held: [bool; 4],
    pressed: [u64; 4],
    presses: u64,
    // The keys taken out for this tick, each with the one that won over it.
    took: Vec<(KeyCode, KeyCode)>,
    // The keys that lost last tick, each with the one that won (Debug Report).
    last: Vec<(KeyCode, KeyCode)>,
}

static SNAP: Mutex<Option<Snap>> = Mutex::new(None);

fn snap<R>(f: impl FnOnce(&mut Snap) -> R) -> R {
    f(SNAP.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Snap::default))
}

// The key of each pair that loses to the one pressed after it, with Snap Tap on; held: in the game's keys or on the
// keyboard (behind a screen GUI Move walks you by the keyboard's). A pair pressed in the same tick stays neutral.
fn losers(held: [bool; 4], pressed: [u64; 4], forward_back: bool) -> Vec<(usize, usize)> {
    let pairs: &[(usize, usize)] = if forward_back { &[(0, 1), (2, 3)] } else { &[(2, 3)] };
    pairs
        .iter()
        .filter(|&&(a, b)| held[a] && held[b] && pressed[a] != pressed[b])
        .map(|&(a, b)| if pressed[a] < pressed[b] { (a, b) } else { (b, a) })
        .collect()
}

// Before the tick: the presses seen since the last one counted, then the losing keys taken out. Returns them, for GUI
// Move's keys behind a screen.
pub(super) fn pre_tick(game: &mut Game<'_>, m: Option<&Module>) -> Vec<KeyCode> {
    let held = KEYS.map(|k| game.keys.contains(&k) || game.physical_keys.contains(&k));
    let order = snap(|s| {
        // Keys first seen in the same tick share their count.
        let new = std::array::from_fn::<bool, 4, _>(|i| held[i] && !s.held[i]);
        if new.contains(&true) {
            s.presses += 1;
        }
        for i in (0..4).filter(|&i| new[i]) {
            s.pressed[i] = s.presses;
        }
        s.held = held;
        s.pressed
    });
    let Some(m) = m.filter(|m| m.p("Mode") == "Last input") else {
        snap(|s| s.last.clear());
        return Vec::new();
    };
    let lost: Vec<(KeyCode, KeyCode)> = losers(held, order, m.b("Forward and back")).into_iter().map(|(l, w)| (KEYS[l], KEYS[w])).collect();
    let took = lost.iter().filter(|(k, _)| game.keys.remove(k)).copied().collect();
    snap(|s| {
        s.took = took;
        s.last.clone_from(&lost);
    });
    lost.into_iter().map(|(k, _)| k).collect()
}

// The keys that lost last tick, each with the one that won over it.
pub(super) fn last() -> Vec<(KeyCode, KeyCode)> {
    snap(|s| s.last.clone())
}

// After the tick: a key taken out goes back while the one that won over it is still held (a tick that let go of every
// key, a death or a screen, keeps it out).
pub(super) fn post_tick(game: &mut Game<'_>) {
    for (key, winner) in snap(|s| std::mem::take(&mut s.took)) {
        if game.keys.contains(&winner) {
            game.keys.insert(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::losers;

    #[test]
    fn the_last_pressed_of_a_pair_wins() {
        // A held, then D: A loses. D let go: nothing loses, A moves you again.
        assert_eq!(losers([false, false, true, true], [0, 0, 1, 2], true), vec![(2, 3)]);
        assert_eq!(losers([false, false, true, false], [0, 0, 1, 2], true), vec![]);
        // D held, then A: D loses.
        assert_eq!(losers([false, false, true, true], [0, 0, 4, 3], true), vec![(3, 2)]);
        // W then S: W loses, unless forward and back are left out.
        assert_eq!(losers([true, true, false, false], [5, 6, 0, 0], true), vec![(0, 1)]);
        assert_eq!(losers([true, true, false, false], [5, 6, 0, 0], false), vec![]);
        // Pressed in the same tick: neutral.
        assert_eq!(losers([false, false, true, true], [0, 0, 7, 7], true), vec![]);
    }
}
