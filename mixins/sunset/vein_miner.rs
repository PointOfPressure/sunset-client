// Vein Miner (compkrx's): break an ore by hand and let go of the button, and the same ore touching it by a face is mined
// too, one block at a time while it is in reach and in sight, up to Max blocks counting the first. Deepslate and stone
// kinds of an ore count as one. Each dig is the game's own: START, a swing every tick, STOP once the block's progress
// is full (Fast finish: at the server's 0.7, like Packet Mine; anticheats such as Grim count that as fast breaking).
// Rotate has the server see you look at a block before its START and STOP. A click, another slot, a menu, using an item
// or Packet Mine ends the vein, and a dig the server is still counting is dropped (PlayerAction 0 start, 2 abort, 3
// stop). No movement packets are sent.
use super::sunset::{Category, Module, cancel_turns, gui_open, with};
use super::sunset_util::{center_of, sequence, turned_to};
use super::{Dim, Game};
use horizon_client::net::sunset_packets::{Link, Linked};
use horizon_core::geometry::Direction;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{PlayerAction, Serverbound};
use std::collections::HashSet;
use std::sync::Mutex;

pub(super) const NAME: &str = "Vein Miner";
// Ticks to wait for the server to clear a block, or for a turn to go out.
const WAIT: u64 = 20;
type Pos = [i32; 3];

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new(NAME, Category::World, "Mines the ore touching an ore you break.")
        .num("Max blocks", 16.0, 1.0, 32.0, 1.0)
        .bool("Rotate", true)
        .bool("Fast finish", false)]
}

struct Vein {
    link: Link,
    life: (i32, Dim),
    ore: String,
    slot: usize,
    left: usize,
    rotate: bool,
    finish: f32,
    phase: Phase,
    seen: HashSet<Pos>,
    next: Vec<Pos>,
}

enum Phase {
    // The ore broken by hand: waits for the button to come up and the block to go.
    Hand(Pos, Option<u64>),
    Scan,
    // START asked for; waits for it to go out.
    Start(Pos, u64),
    Dig(Pos, f32),
    // STOP asked for; waits for the block to go.
    Stop(Pos, u64),
}

static VEIN: Mutex<Option<Vein>> = Mutex::new(None);
// The last dig action that went out: block, face, action.
static SENT: Mutex<Option<(Pos, u8, i32)>> = Mutex::new(None);

fn sent() -> std::sync::MutexGuard<'static, Option<(Pos, u8, i32)>> {
    SENT.lock().unwrap_or_else(|e| e.into_inner())
}

fn current() -> std::sync::MutexGuard<'static, Option<Vein>> {
    VEIN.lock().unwrap_or_else(|e| e.into_inner())
}

fn loaded(game: &Game<'_>, [x, _, z]: Pos) -> bool {
    game.world.chunks.contains_key(&(x >> 4, z >> 4))
}

fn block<'a>(game: &'a Game<'_>, pos: Pos) -> Option<&'a str> {
    loaded(game, pos).then(|| game.res.facts().get(game.world.block(pos[0], pos[1], pos[2])).name.as_ref())
}

// An ore's kind: diamond_ore for diamond_ore and deepslate_diamond_ore; ancient debris counts.
fn kind(name: &str) -> Option<&str> {
    let name = name.strip_prefix("minecraft:")?;
    let name = name.strip_prefix("deepslate_").unwrap_or(name);
    (name.ends_with("_ore") || name == "ancient_debris").then_some(name)
}

// The held item drops it, and the server lets you break there.
fn harvests(game: &Game<'_>, pos: Pos) -> bool {
    let facts = game.res.facts().get(game.world.block(pos[0], pos[1], pos[2]));
    !game.block_action_restricted(pos) && game.items().has_correct_tool_for_drops(game.held(), facts.requires_tool, &facts.name)
}

fn gone(game: &Game<'_>, pos: Pos) -> bool {
    loaded(game, pos) && game.res.facts().is_air(game.world.block(pos[0], pos[1], pos[2]))
}

// The face a line from the eye to the block's centre comes in by, when nothing is in the way and it is in reach.
fn face_in_reach(game: &Game<'_>, pos: Pos) -> Option<u8> {
    let eye = game.eye_position()?;
    let center = center_of(pos);
    let delta: [f64; 3] = std::array::from_fn(|i| center[i] - eye[i]);
    let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    if length == 0.0 {
        return None;
    }
    let hit = game.clip_ray(eye, delta.map(|d| d / length), game.reach().min(length + 0.01))?;
    if hit.pos != pos {
        return None;
    }
    Direction::ALL.iter().position(|d| d.normal() == hit.face).map(|i| i as u8)
}

// The arm swings, and the server hears it (Punch), as each tick of the game's own mining does.
fn swing(game: &mut Game<'_>) {
    game.swing_arm();
    game.send(Serverbound::Punch(Empty));
}

// Sends a dig action with a swing, after the server's look has turned to the block when `rotate` says so.
fn dig(game: &mut Game<'_>, rotate: bool, action: i32, pos: Pos, face: u8) {
    turned_to(game, NAME, rotate, center_of(pos), move |game| {
        let sequence = sequence(game);
        game.send(Serverbound::PlayerAction(PlayerAction { action, pos, direction: face, sequence }));
        swing(game);
        *sent() = Some((pos, face, action));
    });
}

impl Vein {
    fn same_life(&self, game: &Game<'_>) -> bool {
        game.remote.as_ref().is_some_and(|r| {
            self.link.to(Some(&r.session)) == Linked::Same && self.life == (r.entity_id, game.dim)
        })
    }

    // Drops what is still asked for, and a dig the server is counting.
    fn cancel(self, game: &mut Game<'_>) {
        cancel_turns(NAME);
        if let Some((pos, face, 0)) = sent().take()
            && self.same_life(game)
        {
            let sequence = sequence(game);
            game.send(Serverbound::PlayerAction(PlayerAction { action: 2, pos, direction: face, sequence }));
        }
    }

    fn broken(&mut self, [x, y, z]: Pos) {
        self.phase = Phase::Scan;
        self.left = self.left.saturating_sub(1);
        for side in Direction::ALL {
            let [dx, dy, dz] = side.normal();
            let pos = [x + dx, y + dy, z + dz];
            if self.seen.insert(pos) {
                self.next.push(pos);
            }
        }
    }

    fn ore_at(&self, game: &Game<'_>, pos: Pos) -> bool {
        block(game, pos).and_then(kind) == Some(self.ore.as_str())
    }

    // One tick of the vein; false when it is over.
    fn step(&mut self, game: &mut Game<'_>) -> bool {
        match self.phase {
            Phase::Hand(pos, ref mut up) => {
                if game.left {
                    return true;
                }
                if !gone(game, pos) {
                    let until = *up.get_or_insert(game.ticks + WAIT);
                    return game.ticks < until;
                }
                // The next START waits a tick: never two digs in one tick.
                self.broken(pos);
                return self.left > 0;
            }
            Phase::Start(pos, until) => {
                if sent().is_some_and(|(at, _, action)| at == pos && action == 0) {
                    self.phase = Phase::Dig(pos, 0.0);
                    return true;
                }
                return game.ticks < until;
            }
            Phase::Dig(pos, progress) => {
                if gone(game, pos) {
                    self.broken(pos);
                    return self.left > 0;
                }
                if !self.ore_at(game, pos) || !harvests(game, pos) {
                    return false;
                }
                let Some(face) = face_in_reach(game, pos) else { return false };
                let progress = progress + game.destroy_progress(pos);
                if progress >= self.finish {
                    dig(game, self.rotate, 3, pos, face);
                    self.phase = Phase::Stop(pos, game.ticks + WAIT);
                } else {
                    swing(game);
                    self.phase = Phase::Dig(pos, progress);
                }
                return true;
            }
            Phase::Stop(pos, until) => {
                if !gone(game, pos) {
                    return game.ticks < until;
                }
                *sent() = None;
                self.broken(pos);
            }
            Phase::Scan => {}
        }
        if self.left == 0 {
            return false;
        }
        // A block in the way stays on the list: the next break may open it up.
        let found = self.next.iter().enumerate().find_map(|(i, &pos)| {
            if !self.ore_at(game, pos) || !harvests(game, pos) {
                return None;
            }
            Some((i, pos, face_in_reach(game, pos)?))
        });
        let Some((i, pos, face)) = found else { return false };
        self.next.remove(i);
        dig(game, self.rotate, 0, pos, face);
        self.phase = Phase::Start(pos, game.ticks + WAIT);
        true
    }
}

pub(super) fn stop(game: &mut Game<'_>) {
    let active = current().take();
    if let Some(active) = active {
        active.cancel(game);
    }
}

// A click on a block: an ore the held item harvests starts a vein, which waits for the hand's break.
pub(super) fn clicked(game: &mut Game<'_>) {
    stop(game);
    let Some((max, rotate, fast)) = with(|s| {
        s.module(NAME).filter(|m| m.on).map(|m| (m.n("Max blocks") as usize, m.b("Rotate"), m.b("Fast finish")))
    }) else {
        return;
    };
    let (Some(remote), Some(hit)) = (&game.remote, game.pick()) else { return };
    if game.creative || game.entity_targeted() || !harvests(game, hit.pos) {
        return;
    }
    let Some(ore) = block(game, hit.pos).and_then(kind) else { return };
    let vein = Vein {
        link: Link::of(&remote.session),
        life: (remote.entity_id, game.dim),
        ore: ore.to_owned(),
        slot: game.selected,
        left: max.max(1),
        rotate,
        finish: if fast { 0.7 } else { 1.0 },
        phase: Phase::Hand(hit.pos, None),
        seen: HashSet::from([hit.pos]),
        next: Vec::new(),
    };
    *sent() = None;
    *current() = Some(vein);
}

pub(super) fn tick(game: &mut Game<'_>, packet_mine: bool) {
    let Some(mut active) = current().take() else { return };
    if active.same_life(game)
        && game.native_playing() && game.player.is_some() && !gui_open()
        && !game.creative && !game.spectator() && game.using.is_none()
        && game.selected == active.slot && !packet_mine
        && (!game.left || matches!(active.phase, Phase::Hand(..)))
        && active.step(game)
    {
        *current() = Some(active);
    } else {
        active.cancel(game);
    }
}
