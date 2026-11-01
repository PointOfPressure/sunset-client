// Packet Mine (Meteor's): a click on a block queues it, and it is mined by packets in the background: START, then
// STOP once the server's break time has passed (its progress at 0.7, as the server counts it, with the best tool in
// the hotbar), the best tool held only for the STOP tick. You can walk, look away or click more blocks meanwhile.
use super::sunset::{Category, Module, hold, with};
use super::sunset_util::{center_of, dist, sequence, show, turned_to};
use super::Game;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;

const NAME: &str = "Packet Mine";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::World, "Click a block and it is mined in the background by packets: started at once, finished when the server's break time is up, with the fastest tool in your hotbar held only for that tick (Best tool). Click more blocks to queue them; walk or look away meanwhile. Not on use: no tool swap while you eat or block. A box shows each block, red to green as it gets there.")
            .num("Delay (ticks)", 1.0, 0.0, 10.0, 1.0)
            .bool("Rotate", true)
            .bool("Best tool", true)
            .bool("Not on use", true)
            .bool("Render", true)
            .num("Queue", 8.0, 1.0, 32.0, 1.0),
    ]
}

#[derive(Clone, Copy)]
struct Block {
    pos: [i32; 3],
    direction: u8,
    state: horizon_world::registry::BlockStateId,
    // Ticks left before START, then the progress so far and the tick STOP went (waiting for the block to go).
    wait: u32,
    done: Option<f32>,
    stopped: Option<u64>,
}

#[derive(Default)]
struct State {
    queue: Vec<Block>,
    // The slot to go back to the tick after a STOP with the best tool.
    back: Option<usize>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on).cloned())
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
            s.queue.push(Block { pos: hit.pos, direction, state, wait: m.n("Delay (ticks)") as u32, done: None, stopped: None });
        }
    });
    if swing {
        game.swing_arm();
        game.send(S::Punch(Empty));
    }
    true
}

// What one tick of digging with hotbar slot `slot` adds (the game's own sum: the tool, its enchantments, haste, in
// water, off the ground).
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

pub(super) fn tick(game: &mut Game<'_>) {
    if let Some(slot) = st(|s| s.back.take()) {
        hold(game, slot);
    }
    let Some(m) = module() else {
        st(|s| s.queue.clear());
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let ticks = game.ticks;
    let reach = game.reach() + 1.0;
    // Gone (broken, by us or anyone), out of reach, or a second after its STOP: off the queue.
    st(|s| {
        s.queue.retain(|b| {
            game.world.block(b.pos[0], b.pos[1], b.pos[2]) == b.state
                && dist(eye, center_of(b.pos)) <= reach
                && b.stopped.is_none_or(|t| ticks < t + 20)
        })
    });
    if m.b("Render") {
        for b in st(|s| s.queue.clone()) {
            let p = b.done.map_or(0.0, |d| (d / 0.7).min(1.0));
            let (r, g) = ((255.0 * (1.0 - p)) as u32, (255.0 * p) as u32);
            show(b.pos, 0x6000_0000 | r << 16 | g << 8);
        }
    }
    let Some(mut b) = st(|s| s.queue.first().copied()) else {
        return;
    };
    if b.stopped.is_some() {
        return;
    }
    let rotate = m.b("Rotate");
    match b.done {
        None if b.wait > 0 => b.wait -= 1,
        None => {
            b.done = Some(0.0);
            let (pos, direction) = (b.pos, b.direction);
            turned_to(game, NAME, rotate, center_of(pos), move |game| {
                let sequence = sequence(game);
                game.send(S::PlayerAction(play::PlayerAction { action: 0, pos, direction, sequence }));
                game.swing_arm();
            });
        }
        Some(done) => {
            let slot = best_slot(game, &m, b.pos);
            let done = done + progress_with(game, slot, b.pos);
            b.done = Some(done);
            // The server lets a STOP through once its progress reaches 0.7, counting the tick it started in.
            if done >= 0.7 {
                b.stopped = Some(ticks);
                let (pos, direction) = (b.pos, b.direction);
                turned_to(game, NAME, rotate, center_of(pos), move |game| {
                    let before = game.selected;
                    hold(game, slot);
                    let sequence = sequence(game);
                    game.send(S::PlayerAction(play::PlayerAction { action: 3, pos, direction, sequence }));
                    if slot != before {
                        st(|s| s.back = Some(before));
                    }
                });
            }
        }
    }
    st(|s| {
        if let Some(first) = s.queue.first_mut().filter(|f| f.pos == b.pos) {
            *first = b;
        }
    });
}

// Turned off: a block the server is digging stops there too. (PlayerAction 0 starts a dig, 2 drops it, 3 finishes
// it.)
pub(super) fn toggled(game: &mut Game<'_>, name: &str, on: bool) {
    if name != NAME || on {
        return;
    }
    let digging = st(|s| std::mem::take(&mut s.queue)).into_iter().find(|b| b.done.is_some() && b.stopped.is_none());
    if let Some(b) = digging {
        game.send(S::PlayerAction(play::PlayerAction { action: 2, pos: b.pos, direction: b.direction, sequence: 0 }));
    }
}
