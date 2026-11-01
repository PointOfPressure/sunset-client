"""Test the exact runtime rotation claim/cancel/deed code without a renderer."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'mixins/sunset/runtime.rs').read_text(encoding='utf-8')
start = source.index('type Deed = ')
end = source.index('// Has the server see the player look', start)
helper = source[start:end]
turned = source[source.index('fn turned(game:'):source.index('// The end of the tick:', source.index('fn turned(game:'))]
tests = r'''
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
static BLINK: AtomicBool = AtomicBool::new(false);
pub(super) struct Game<'a> { calls: usize, marker: std::marker::PhantomData<&'a ()> }
fn clean() {
    BLINK.store(false, Ordering::Relaxed);
    turns(|t| { t.queue.clear(); t.carried = false; t.restore = false; });
}
#[test]
fn two_auras_cannot_claim_the_same_turn_and_rejected_deeds_never_run() {
    clean();
    assert!(try_turn_then("Crystal Aura", 400.0, 200.0, |g| g.calls += 1));
    assert!(!try_turn_then("Anchor Aura", 0.0, 0.0, |g| g.calls += 100));
    assert!(!try_turn_then("Crystal Aura", 0.0, 0.0, |g| g.calls += 100));
    turns(|t| {
        assert_eq!(t.queue[0].look, (40.0, 90.0));
        assert!(t.queue[0].fresh);
        t.carried = true;
    });
    let mut game = Game { calls: 0, marker: std::marker::PhantomData };
    turned(&mut game);
    turned(&mut game);
    assert_eq!(game.calls, 1);
    assert!(try_turn_then("Anchor Aura", 0.0, 0.0, |g| g.calls += 2));
    turns(|t| t.carried = true);
    turned(&mut game);
    assert_eq!(game.calls, 3);
}
#[test]
fn cancellation_preserves_other_modules_and_rotation_restore() {
    clean();
    assert!(try_turn_then("Scaffold", 0.0, 0.0, |_| {}));
    cancel_turns("Anchor Aura");
    assert!(!try_turn_then("Crystal Aura", 0.0, 0.0, |_| {}));
    turns(|t| { assert_eq!(t.queue.len(), 1); t.carried = true; t.restore = true; });
    cancel_turns("Scaffold");
    turns(|t| { assert!(t.queue.is_empty()); assert!(!t.carried); assert!(t.restore); });
    let mut game = Game { calls: 0, marker: std::marker::PhantomData };
    turned(&mut game);
    assert_eq!(game.calls, 0);
}
#[test]
fn invalid_angles_blink_and_in_flight_turns_reject_without_side_effects() {
    clean();
    for (yaw, pitch) in [(f32::NAN, 0.0), (0.0, f32::INFINITY)] {
        assert!(!try_turn_then("Crystal Aura", yaw, pitch, |_| panic!("rejected deed")));
    }
    BLINK.store(true, Ordering::Relaxed);
    assert!(!try_turn_then("Anchor Aura", 0.0, 0.0, |_| panic!("blink deed")));
    BLINK.store(false, Ordering::Relaxed);
    turns(|t| t.carried = true);
    assert!(!try_turn_then("Anchor Aura", 0.0, 0.0, |_| panic!("in-flight deed")));
    turns(|t| assert!(t.queue.is_empty()));
}
'''
with tempfile.TemporaryDirectory(prefix='sunset-turn-tests-') as directory:
    path = Path(directory) / 'turns.rs'
    path.write_text('mod harness {\n' + tests + '\n' + helper + '\n' + turned + '\n}', encoding='utf-8')
    binary = Path(directory) / 'turns.exe'
    subprocess.run(['rustc', '--edition=2024', '--test', str(path), '-o', str(binary)], check=True)
    subprocess.run([str(binary), '--test-threads=1'], check=True)
