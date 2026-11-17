use super::sunset::{Category, Module, gui_open, with};
use super::sunset_util::{center_of, sequence};
use super::{Dim, Game};
use horizon_client::net::sunset_packets::{Link, Linked};
use horizon_core::geometry::Direction;
use horizon_protocol::packets::play::{PlayerAction, Serverbound};
use std::collections::HashSet;
use std::sync::Mutex;

pub(super) const NAME: &str = "Vein Miner";
const ACK_TICKS: u64 = 20;
type Pos = [i32; 3];

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new(NAME, Category::World, "Mines reachable ore connected to an ore you break.")
        .num("Max blocks", 16.0, 1.0, 32.0, 1.0)]
}

struct Vein {
    link: Link,
    life: (i32, Dim),
    family: String,
    slot: usize,
    remaining: usize,
    phase: Phase,
    seen: HashSet<Pos>,
    frontier: Vec<Pos>,
}

enum Phase {
    Seed(Pos, Option<u64>),
    Scan,
    Dig { pos: Pos, face: u8, progress: f32 },
    AwaitBlock(Pos, u64),
}

static VEIN: Mutex<Option<Vein>> = Mutex::new(None);

fn block<'a>(game: &'a Game<'_>, [x, y, z]: Pos) -> Option<&'a str> {
    game.world.chunks.contains_key(&(x >> 4, z >> 4))
        .then(|| game.res.facts().get(game.world.block(x, y, z)).name.as_ref())
}

fn family(name: &str) -> Option<&str> {
    let name = name.strip_prefix("minecraft:")?;
    let name = name.strip_prefix("deepslate_").unwrap_or(name);
    (name.ends_with("_ore") || name == "ancient_debris").then_some(name)
}

fn harvests(game: &Game<'_>, pos: Pos) -> bool {
    let facts = game.res.facts().get(game.world.block(pos[0], pos[1], pos[2]));
    !game.block_action_restricted(pos)
        && game.items().has_correct_tool_for_drops(game.held(), facts.requires_tool, &facts.name)
}

fn air(game: &Game<'_>, [x, y, z]: Pos) -> bool {
    game.world.chunks.contains_key(&(x >> 4, z >> 4))
        && game.res.facts().is_air(game.world.block(x, y, z))
}

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

fn action(game: &mut Game<'_>, action: i32, pos: Pos, direction: u8) {
    let sequence = sequence(game);
    game.send(Serverbound::PlayerAction(PlayerAction { action, pos, direction, sequence }));
}

impl Vein {
    fn same_life(&self, game: &Game<'_>) -> bool {
        game.remote.as_ref().is_some_and(|r| {
            self.link.to(Some(&r.session)) == Linked::Same && self.life == (r.entity_id, game.dim)
        })
    }

    fn cancel(self, game: &mut Game<'_>) {
        if let Phase::Dig { pos, face, .. } = self.phase
            && self.same_life(game)
        {
            action(game, 2, pos, face);
        }
    }

    fn broken(&mut self, [x, y, z]: Pos) {
        self.phase = Phase::Scan;
        self.remaining -= 1;
        for side in Direction::ALL {
            let [dx, dy, dz] = side.normal();
            let next = [x + dx, y + dy, z + dz];
            if self.seen.insert(next) {
                self.frontier.push(next);
            }
        }
    }

    fn matches(&self, game: &Game<'_>, pos: Pos) -> bool {
        block(game, pos).and_then(family) == Some(self.family.as_str())
    }

    fn step(&mut self, game: &mut Game<'_>) -> bool {
        match self.phase {
            Phase::Seed(pos, ref mut released) => {
                // Let the normal attack finish and the player's mining input go idle.
                if game.left {
                    return true;
                }
                if air(game, pos) {
                    self.broken(pos);
                } else {
                    let end = *released.get_or_insert(game.ticks + ACK_TICKS);
                    return game.ticks < end;
                }
            }
            Phase::Dig { pos, face, progress } => {
                if air(game, pos) {
                    self.broken(pos);
                    return self.remaining > 0;
                }
                if !self.matches(game, pos) || face_in_reach(game, pos).is_none() || !harvests(game, pos) {
                    return false;
                }
                let progress = progress + game.destroy_progress(pos);
                // Same server-side STOP threshold as Packet Mine.
                self.phase = if progress >= 0.7 {
                    action(game, 3, pos, face);
                    Phase::AwaitBlock(pos, game.ticks + ACK_TICKS)
                } else {
                    Phase::Dig { pos, face, progress }
                };
                return true;
            }
            Phase::AwaitBlock(pos, end) => {
                if !air(game, pos) {
                    return game.ticks < end;
                }
                self.broken(pos);
            }
            Phase::Scan => {}
        }

        if self.remaining == 0 {
            return false;
        }
        // Keep obstructed neighbors until the next break, which may expose them.
        let target = self.frontier.iter().enumerate().find_map(|(i, &pos)| {
            if !self.matches(game, pos) || !harvests(game, pos) {
                return None;
            }
            Some((i, pos, face_in_reach(game, pos)?))
        });
        let Some((i, pos, face)) = target else { return false };
        self.frontier.remove(i);
        action(game, 0, pos, face);
        game.swing_arm();
        self.phase = Phase::Dig { pos, face, progress: 0.0 };
        true
    }
}

pub(super) fn stop(game: &mut Game<'_>) {
    let active = VEIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(active) = active {
        active.cancel(game);
    }
}

pub(super) fn observe(game: &Game<'_>) {
    let Some(remaining) = with(|s| s.module(NAME).filter(|m| m.on).map(|m| m.n("Max blocks") as usize)) else { return };
    let (Some(remote), Some(hit)) = (&game.remote, game.pick()) else { return };
    if game.creative || game.entity_targeted() || !harvests(game, hit.pos) {
        return;
    }
    let Some(family) = block(game, hit.pos).and_then(family) else { return };
    *VEIN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vein {
        link: Link::of(&remote.session),
        life: (remote.entity_id, game.dim),
        family: family.to_owned(),
        slot: game.selected,
        remaining,
        phase: Phase::Seed(hit.pos, None),
        seen: HashSet::from([hit.pos]),
        frontier: Vec::new(),
    });
}

pub(super) fn tick(game: &mut Game<'_>, packet_mine: bool) {
    let Some(mut active) = VEIN.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    if active.same_life(game)
        && game.native_playing() && game.player.is_some() && !gui_open()
        && !game.creative && !game.spectator() && game.using.is_none()
        && game.selected == active.slot && !packet_mine
        && (!game.left || matches!(active.phase, Phase::Seed(..)))
        && active.step(game)
    {
        *VEIN.lock().unwrap_or_else(|e| e.into_inner()) = Some(active);
    } else {
        active.cancel(game);
    }
}
