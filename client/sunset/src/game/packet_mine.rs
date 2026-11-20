// Packet Mine (Sunset's): a click on a block queues it, and it is mined by packets in the background: START, then
// STOP once the server takes it (ServerPlayerGameMode: what a tick of digging adds with the item held at the STOP,
// times the ticks since the START, reaching 0.7), the best tool in the hotbar held only for the STOP tick. You can
// walk, look away or click more blocks meanwhile.
use super::sunset::{Category, Module, hold, with};
use super::sunset_util::{center_of, dist, sequence, show, turned_to};
use super::Game;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Clientbound as C, Serverbound as S};
use std::sync::Mutex;

const NAME: &str = "Packet Mine";
// A START or STOP asked for this long and still not sent (its look was dropped or is stuck behind others) is asked
// for again.
const ASK_TICKS: u64 = 20;
// How long a block may take to go after its STOP (the server can finish it alone: its delayed destroy).
const GONE_TICKS: u64 = 20;
// The progress at which the server lets a STOP break the block.
const STOP_AT: f32 = 0.7;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::World, "Mines blocks in the background.")
            .num("Delay (ticks)", 1.0, 0.0, 10.0, 1.0)
            .bool("Rotate", true)
            .bool("Best tool", true)
            .bool("Not on use", true)
            .bool("Render", true)
            .num("Queue", 8.0, 1.0, 32.0, 1.0),
    ]
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    // Ticks left before the START is asked for.
    Wait(u32),
    // START asked for at this tick, waiting for its look.
    Starting(u64),
    // START sent at this tick: the server's dig clock starts there.
    Digging(u64),
    // The dig's START tick, and the tick its STOP was asked for.
    Stopping(u64, u64),
    // STOP sent (or the START broke it at once) at this tick: waiting for the block to go.
    Done(u64),
}

impl Phase {
    // The server is digging the block: leaving it takes an ABORT.
    fn dug(self) -> bool {
        matches!(self, Phase::Digging(_) | Phase::Stopping(_, _))
    }
}

#[derive(Clone, Copy)]
struct Block {
    pos: [i32; 3],
    direction: u8,
    state: horizon_world::registry::BlockStateId,
    phase: Phase,
}

// The connection, player and dimension a queue belongs to.
type Scene = (usize, i32, super::Dim);

#[derive(Default)]
struct State {
    queue: Vec<Block>,
    // After a STOP with the best tool: the slot to go back to the next tick, and the tool's slot.
    back: Option<(usize, usize)>,
    scene: Option<Scene>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on).cloned())
}

fn scene(game: &Game<'_>) -> Option<Scene> {
    game.remote
        .as_ref()
        .map(|r| (std::sync::Arc::as_ptr(&r.session.counters) as *const () as usize, r.entity_id, game.dim))
}

// The best tool is held for a STOP and given back the next tick: Crystal Aura and Anchor Aura leave the hand alone
// until then, so the swap doesn't look to them like the hotbar taken from them.
pub(super) fn borrowed() -> bool {
    st(|s| s.back.is_some())
}

// A click (`swing`, as the game's own click swings) or the button held on a block: the block under the crosshair goes
// in the queue instead of being mined the game's way. True: the click was taken.
pub(super) fn click(game: &mut Game<'_>, swing: bool) -> bool {
    let Some(m) = module() else {
        return false;
    };
    if game.creative || game.remote.is_none() || game.entity_targeted() {
        return false;
    }
    let Some(hit) = game.pick() else {
        return false;
    };
    let state = game.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
    let facts = game.res.facts().get(state);
    if facts.hardness < 0.0 || game.res.facts().is_air(state) {
        return false;
    }
    let direction = horizon_core::geometry::Direction::ALL.iter().position(|d| d.normal() == hit.face).unwrap_or(1) as u8;
    st(|s| {
        if !s.queue.iter().any(|b| b.pos == hit.pos) && s.queue.len() < m.n("Queue") as usize {
            let wait = m.n("Delay (ticks)") as u32;
            s.queue.push(Block { pos: hit.pos, direction, state, phase: Phase::Wait(wait) });
        }
    });
    if swing {
        game.swing_arm();
        game.send(S::Punch(Empty));
    }
    true
}

// What one tick of digging with hotbar slot `slot` adds (the game's own sum: the tool, its enchantments, haste, in
// water, off the ground), as the server counts it for a STOP sent now.
fn progress_with(game: &mut Game<'_>, slot: usize, pos: [i32; 3]) -> f32 {
    let held = game.selected;
    game.selected = slot;
    let step = game.destroy_progress(pos);
    game.selected = held;
    step
}

fn best_slot(game: &mut Game<'_>, m: &Module, pos: [i32; 3]) -> usize {
    if !m.b("Best tool") || (m.b("Not on use") && game.using.is_some()) {
        return game.selected;
    }
    (0..9).fold(game.selected, |best, i| if progress_with(game, i, pos) > progress_with(game, best, pos) { i } else { best })
}

// The server breaks the block for a STOP when a tick's progress (with the item and the conditions of the STOP's
// tick, not a sum over the dig) times its count of ticks reaches STOP_AT.
fn stop_due(step: f32, ticks: u64) -> bool {
    step > 0.0 && step * ticks as f32 >= STOP_AT
}

fn phase_is(pos: [i32; 3], phase: Phase) -> bool {
    st(|s| s.queue.first().is_some_and(|b| b.pos == pos && b.phase == phase))
}

fn set_phase(pos: [i32; 3], phase: Phase) {
    st(|s| {
        if let Some(b) = s.queue.first_mut().filter(|b| b.pos == pos) {
            b.phase = phase;
        }
    });
}

// PlayerAction 0 starts a dig, 2 drops it, 3 finishes it; each with the game's block-action sequence number.
fn abort(game: &mut Game<'_>, b: &Block) {
    let sequence = sequence(game);
    game.send(S::PlayerAction(play::PlayerAction { action: 2, pos: b.pos, direction: b.direction, sequence }));
}

// The queue goes; a block the server is digging (and still there) stops there too.
fn drop_queue(game: &mut Game<'_>) {
    let digging: Vec<Block> = st(|s| std::mem::take(&mut s.queue)).into_iter().filter(|b| b.phase.dug()).collect();
    for b in &digging {
        if !game.res.facts().is_air(game.world.block(b.pos[0], b.pos[1], b.pos[2])) {
            abort(game, b);
        }
    }
}

pub(super) fn tick(game: &mut Game<'_>) {
    // Another world, life or connection: what was queued (and the server's dig) belongs to the old one.
    let now = scene(game);
    if st(|s| s.scene != now) {
        st(|s| {
            s.queue.clear();
            s.back = None;
            s.scene = now;
        });
    }
    if let Some((before, tool)) = st(|s| s.back.take()) {
        // Unless the slot was changed since (by you or another module).
        if game.selected == tool {
            hold(game, before);
        }
    }
    let Some(m) = module() else {
        drop_queue(game);
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let ticks = game.ticks;
    let reach = game.reach() + 1.0;
    // Gone (broken, by us or anyone), changed, out of reach, or a second after its STOP: off the queue, and a dig the
    // server still has goes with an ABORT.
    let mut left = Vec::new();
    st(|s| {
        s.queue.retain(|b| {
            let state = game.world.block(b.pos[0], b.pos[1], b.pos[2]);
            let keep = state == b.state
                && dist(eye, center_of(b.pos)) <= reach
                && !matches!(b.phase, Phase::Done(t) if ticks >= t + GONE_TICKS);
            if !keep && b.phase.dug() && !game.res.facts().is_air(state) {
                left.push(*b);
            }
            keep
        });
        // A look that never went out leaves its START or STOP unsent: asked for again (the old one, should it still
        // go, finds its phase gone and does nothing).
        if let Some(b) = s.queue.first_mut() {
            match b.phase {
                Phase::Starting(asked) if ticks >= asked + ASK_TICKS => b.phase = Phase::Wait(0),
                Phase::Stopping(start, asked) if ticks >= asked + ASK_TICKS => b.phase = Phase::Digging(start),
                _ => {}
            }
        }
    });
    for b in &left {
        abort(game, b);
    }
    let Some(b) = st(|s| s.queue.first().copied()) else {
        return;
    };
    let rotate = m.b("Rotate");
    let (pos, direction) = (b.pos, b.direction);
    let mut shown = 0.0;
    match b.phase {
        Phase::Wait(n) if n > 0 => set_phase(pos, Phase::Wait(n - 1)),
        Phase::Wait(_) => {
            let asked = Phase::Starting(ticks);
            set_phase(pos, asked);
            turned_to(game, NAME, rotate, center_of(pos), move |game| {
                if !phase_is(pos, asked) {
                    return;
                }
                let sequence = sequence(game);
                game.send(S::PlayerAction(play::PlayerAction { action: 0, pos, direction, sequence }));
                game.swing_arm();
                // A block one tick breaks (with the item held now) goes at the START, and the server wants no STOP for it.
                let at_once = progress_with(game, game.selected, pos) >= 1.0;
                set_phase(pos, if at_once { Phase::Done(game.ticks) } else { Phase::Digging(game.ticks) });
            });
        }
        Phase::Digging(start) => {
            let slot = best_slot(game, &m, pos);
            let step = progress_with(game, slot, pos);
            // One tick short of the server's count (the ticks since the START, plus one): room for a STOP that gets
            // there a tick early.
            let since = ticks.saturating_sub(start);
            shown = (step * since as f32 / STOP_AT).min(1.0);
            // A combat module's queued hand action has the hotbar first; the STOP waits for it.
            if stop_due(step, since) && super::sunset_combat_control::owner().is_none() {
                let asked = Phase::Stopping(start, ticks);
                set_phase(pos, asked);
                let m = m.clone();
                turned_to(game, NAME, rotate, center_of(pos), move |game| {
                    if !phase_is(pos, asked) {
                        return;
                    }
                    // Checked again as the server will count it, with this tick's tool, ground and water.
                    let slot = best_slot(game, &m, pos);
                    if !stop_due(progress_with(game, slot, pos), game.ticks.saturating_sub(start) + 1)
                        || super::sunset_combat_control::owner().is_some()
                    {
                        set_phase(pos, Phase::Digging(start));
                        return;
                    }
                    let before = game.selected;
                    hold(game, slot);
                    let sequence = sequence(game);
                    game.send(S::PlayerAction(play::PlayerAction { action: 3, pos, direction, sequence }));
                    set_phase(pos, Phase::Done(game.ticks));
                    if slot != before {
                        st(|s| s.back = Some((before, slot)));
                    }
                });
            }
        }
        Phase::Stopping(_, _) | Phase::Done(_) => shown = 1.0,
        Phase::Starting(_) => {}
    }
    if m.b("Render") {
        for b in st(|s| s.queue.clone()) {
            let p = if b.pos == pos { shown } else { 0.0 };
            let (r, g) = ((255.0 * (1.0 - p)) as u32, (255.0 * p) as u32);
            show(b.pos, 0x6000_0000 | r << 16 | g << 8);
        }
    }
}

// A new login or respawn: the server forgot the dig with the old life, and the queue goes with it.
pub(super) fn packet(packet: &C) {
    if matches!(packet, C::Login(_) | C::Respawn(_)) {
        st(|s| {
            s.queue.clear();
            s.back = None;
        });
    }
}

// Turned off: a block the server is digging stops there too.
pub(super) fn toggled(game: &mut Game<'_>, name: &str, on: bool) {
    if name != NAME || on {
        return;
    }
    drop_queue(game);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_waits_for_the_servers_count_of_the_stop_tick() {
        // Obsidian with a diamond pickaxe: about 1/188 a tick on the ground, five times less off it.
        let ground = 1.0 / 188.0;
        assert!(!stop_due(ground, 131));
        assert!(stop_due(ground, 132));
        assert!(!stop_due(ground / 5.0, 132));
        assert!(!stop_due(0.0, u64::MAX));
    }

    #[test]
    fn only_a_dig_the_server_has_needs_an_abort() {
        assert!(Phase::Digging(3).dug());
        assert!(Phase::Stopping(3, 9).dug());
        assert!(!Phase::Starting(3).dug());
        assert!(!Phase::Wait(0).dug());
        assert!(!Phase::Done(3).dug());
    }
}
