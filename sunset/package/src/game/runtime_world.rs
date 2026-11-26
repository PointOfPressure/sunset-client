// Runtime part (see runtime.rs): Nuker, logouts, waypoints, deaths and holes.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset_util::block_pos;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use super::sunset::*;

// Nuker's box for the Cube shape (Sunset's pos1 and pos2 by the way you face, with its offsets), block corners.
fn nuke_cube(game: &Game<'_>, m: &Module, at: [f64; 3]) -> [[f64; 3]; 2] {
    let n = |k: &str| m.n(k);
    let (x, y, z) = (at[0], at[1], at[2]);
    let (top, bottom) = ((y + n("Up") + 1.0).ceil(), y.ceil() - n("Down"));
    // Yaw 0 faces south (+z), 90 west, 180 north, -90 east.
    let facing = ((f64::from(game.yaw) / 90.0).round() as i32).rem_euclid(4);
    let (lo, hi) = match facing {
        0 => ([x + 1.0 - (n("Right") + 1.0), z + 1.0 - (n("Back") + 1.0)], [x + 1.0 + n("Left"), z + 1.0 + n("Forward")]),
        1 => ([x - n("Forward"), z - n("Right")], [x + n("Back") + 1.0, z + n("Left") + 1.0]),
        2 => ([x + 1.0 - (n("Left") + 1.0), z + 1.0 - (n("Forward") + 1.0)], [x + 1.0 + n("Right"), z + 1.0 + n("Back")]),
        _ => ([x + 1.0 - (n("Back") + 1.0), z - n("Left")], [x + 1.0 + n("Forward"), z + n("Right") + 1.0]),
    };
    let bottom = if m.p("Mode") == "Flatten" { (y + 0.5).floor() } else { bottom };
    // BlockPos.set floors each coordinate; the box runs between the two blocks' middles.
    let lo = [lo[0].floor() + 0.5, bottom.floor() + 0.5, lo[1].floor() + 0.5];
    let hi = [hi[0].floor() + 0.5, top.floor() + 0.5, hi[1].floor() + 0.5];
    [std::array::from_fn(|i| lo[i].min(hi[i])), std::array::from_fn(|i| lo[i].max(hi[i]))]
}

// Nuker (Sunset's): every block in its shape (a sphere of Range round your feet, a box by Up, Down and the sides,
// or a cube of whole blocks), above your feet in Flatten mode and only the instantly broken in Smash, within reach
// (behind a block, within Walls range) and the lists allow; sorted, then broken: as many as Max blocks per tick of
// those that break at once, else the first, mined over the ticks it takes (Packet mine starts and stops them all).
// Interact right-clicks them instead.
// How long a block Nuker sent waits for the server's update before it is tried again.
const NUKE_WAIT: u64 = 40;

pub(super) fn nuker(game: &mut Game<'_>, m: &Module) {
    with(|s| {
        s.nuke_broken.retain_mut(|(_, t)| {
            *t = t.saturating_sub(1);
            *t > 0
        })
    });
    if !game.native_playing() || game.remote.is_none() {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    // A block being mined the slow way: stop it when its time is up, and start the next one in the same tick.
    let now = game.ticks;
    if let Some((pos, left)) = with(|s| s.nuke) {
        if left <= 1 {
            let seq = super::sunset_util::sequence(game);
            game.send(S::PlayerAction(play::PlayerAction { action: 3, pos, direction: 1, sequence: seq }));
            // The block is still there until the server's update arrives: leave it alone meanwhile.
            with(|s| {
                s.nuke = None;
                s.nuke_skip.push((pos, now + NUKE_WAIT));
            });
        } else {
            with(|s| s.nuke = Some((pos, left - 1)));
            return;
        }
    }
    if with(|s| {
        let wait = s.nuke_timer > 0;
        s.nuke_timer = s.nuke_timer.saturating_sub(1);
        wait
    }) {
        return;
    }
    // Blocks sent wait for the server's update (the block gone here) or, should the server refuse, NUKE_WAIT ticks.
    let skip = with(|s| {
        let facts = game.res.facts();
        s.nuke_skip.retain(|&(p, until)| until > now && !facts.is_air(game.world.block(p[0], p[1], p[2])));
        s.nuke_skip.iter().map(|&(pos, _)| pos).collect::<Vec<_>>()
    });
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let at = [p.position.x, p.position.y, p.position.z];
    let feet = at.map(|v| v.floor() as i32);
    let (shape, mode, range) = (m.p("Shape"), m.p("Mode"), m.n("Range"));
    let r = range.round() as i32;
    let cube = (shape == "Cube").then(|| nuke_cube(game, m, at));
    with(|s| s.nuke_box = cube);
    let reach = match shape {
        "Uniform cube" => r + 1,
        "Cube" => 1 + ["Up", "Down", "Left", "Right", "Forward", "Back"].iter().map(|k| m.n(k) as i32).max().unwrap_or(0),
        _ => range.ceil() as i32 + 1,
    };
    let (interact, suitable) = (m.b("Interact"), m.b("Only suitable tools"));
    let list = if m.p("List mode") == "Whitelist" { m.t("Whitelist") } else { m.t("Blacklist") };
    let interacted = with(|s| s.nuke_interacted.clone());
    let facts = game.res.facts();
    let mut found: Vec<([i32; 3], bool, u32)> = Vec::new();
    for x in feet[0] - reach..=feet[0] + reach {
        for y in feet[1] - reach..=feet[1] + reach {
            for z in feet[2] - reach..=feet[2] + reach {
                let b = [x, y, z];
                let c = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
                let inside = match shape {
                    "Uniform cube" => (0..3).map(|i| (b[i] - feet[i]).abs()).max().unwrap_or(0) < r,
                    "Cube" => cube.is_some_and(|[lo, hi]| (0..3).all(|i| c[i] >= lo[i] && c[i] <= hi[i])),
                    _ => (0..3).map(|i| (c[i] - at[i]).powi(2)).sum::<f64>() <= range * range,
                };
                if !inside || (mode == "Flatten" && c[1] < at[1]) || skip.contains(&b) {
                    continue;
                }
                let state = game.world.block(x, y, z);
                if facts.is_air(state) {
                    continue;
                }
                let f = facts.get(state);
                if mode == "Smash" && f.hardness != 0.0 {
                    continue;
                }
                if suitable && !interact && !game.res.items().correct_for_drops(game.held(), &f.name) {
                    continue;
                }
                let fluid = f.name.ends_with("water") || f.name.ends_with("lava");
                if !interact && (f.hardness < 0.0 || fluid) {
                    continue;
                }
                // Behind a block (the line to its middle meets another first): only within Walls range.
                if !super::sunset_util::clear_line(game, c, b) && super::sunset_util::dist(eye, c) > m.n("Walls range") {
                    continue;
                }
                if super::sunset_util::listed(list, &f.name) != (m.p("List mode") == "Whitelist") {
                    continue;
                }
                if interact && interacted.contains(&b) {
                    continue;
                }
                let speed = game.res.items().mining_speed(game.held(), &f.name);
                let per_tick = if f.hardness <= 0.0 { 1.0 } else { speed / f.hardness / if speed > 1.0 { 30.0 } else { 100.0 } };
                let instant = game.creative || per_tick >= 1.0;
                if m.b("Instant only") && !instant {
                    continue;
                }
                // The server lets a stop through once the progress reaches 0.7, counting the tick it started in.
                let ticks = if instant { 0 } else { ((0.7 / per_tick).ceil() as u32).max(1) };
                found.push((b, instant, ticks));
            }
        }
    }
    let far = |b: &[i32; 3]| (0..3).map(|i| (f64::from(b[i]) + 0.5 - at[i]).powi(2)).sum::<f64>();
    match m.p("Sort mode") {
        "Top down" => found.sort_by_key(|f| -f.0[1]),
        "Closest" => found.sort_by(|a, b| far(&a.0).total_cmp(&far(&b.0))),
        "Furthest" => found.sort_by(|a, b| far(&b.0).total_cmp(&far(&a.0))),
        _ => {}
    }
    let delay = m.n("Delay") as u32;
    let Some(first) = found.first().map(|f| f.0) else {
        with(|s| {
            s.nuke_interacted.clear();
            s.nuke_idle += 1;
            if s.nuke_idle > delay {
                s.nuke_first = true;
            }
        });
        return;
    };
    let wait = with(|s| {
        s.nuke_idle = 0;
        let mut wait = false;
        if !s.nuke_first && s.nuke_last != Some(first) {
            s.nuke_timer = delay;
            s.nuke_last = Some(first);
            wait = delay > 0;
        }
        wait
    });
    if wait {
        return;
    }
    // No more blocks awaiting the server's answer than four ticks' worth: a server that falls behind is not flooded
    // with blocks sent again and again (Horizon's dropped a client 8000 packets behind), and one that answers a few
    // ticks late (Horizon's, single player's under load) still gets a full tick's worth each tick.
    let (max, packet) = (m.n("Max blocks per tick") as usize, m.b("Packet mine"));
    let max = max.min((4 * max).saturating_sub(skip.len()));
    let mut done = 0;
    let mut sent: Vec<([i32; 3], u64)> = Vec::new();
    // What goes to the server, after the look when Rotate is on.
    let mut out: Vec<S> = Vec::new();
    for (pos, instant, ticks) in found {
        if done >= max {
            break;
        }
        if interact {
            let seq = super::sunset_util::sequence(game);
            let face = if eye[1] > f64::from(pos[1]) + 1.0 { 1 } else { 0 };
            out.push(S::UseItemOn(play::UseItemOn { hand: 0, pos, face, cursor: [0.5, 0.5, 0.5], inside: true, world_border: false, sequence: seq }));
            with(|s| s.nuke_interacted.push(pos));
        } else {
            let seq = super::sunset_util::sequence(game);
            out.push(S::PlayerAction(play::PlayerAction { action: 0, pos, direction: 1, sequence: seq }));
            if packet && !instant {
                let seq = super::sunset_util::sequence(game);
                // 26.x numbers the actions START 0, ABORT 2, STOP 3.
                out.push(S::PlayerAction(play::PlayerAction { action: 3, pos, direction: 1, sequence: seq }));
            } else if !instant {
                with(|s| s.nuke = Some((pos, ticks)));
            }
            // The block shows until the server's update comes back: next tick go on to new ones, not these again.
            sent.push((pos, now + NUKE_WAIT));
        }
        with(|s| {
            s.nuke_last = Some(pos);
            if m.b("Broken blocks") {
                s.nuke_broken.push((pos, 8));
            }
        });
        done += 1;
        if !instant && !packet && !interact {
            break;
        }
    }
    with(|s| {
        s.nuke_skip.extend(sent);
        s.nuke_first = false;
    });
    if done == 0 {
        return;
    }
    let swing = m.b("Swing");
    let send = move |game: &mut Game<'_>| {
        for p in out {
            game.send(p);
        }
        if swing {
            game.swing_arm();
        }
        game.send(S::Punch(Empty));
    };
    if m.b("Rotate") {
        let (yaw, pitch) = look_at(eye, super::sunset_util::center_of(first));
        turn_then("Nuker", yaw, pitch, send);
    } else {
        send(game);
    }
}

// Players who left the list since they were last seen: where they stood.
pub(super) fn track_logouts(game: &Game<'_>, m: &Module) {
    let Some(online) = game.remote_online_names() else {
        return;
    };
    let dim = game.dim.id().to_owned();
    let mut seen: Vec<(String, [f64; 3], String)> = Vec::new();
    for (id, name) in game.remote_player_names() {
        if let Some(mob) = game.mobs.iter().find(|mob| mob.id == id) {
            seen.push((name, [mob.position.x, mob.position.y, mob.position.z], dim.clone()));
        }
    }
    let now = game.ticks;
    let keep = (m.n("Minutes") * 1200.0) as u64;
    with(|s| {
        for (name, pos, d) in s.last_pos.clone() {
            if !online.iter().any(|n| *n == name) {
                s.spots.retain(|sp| sp.name != name);
                s.spots.push(Spot { name, pos, dim: d, at: now });
            }
        }
        s.last_pos.retain(|(n, _, _)| online.contains(n) && !seen.iter().any(|(sn, _, _)| sn == n));
        s.last_pos.extend(seen);
        s.spots.retain(|sp| now.saturating_sub(sp.at) < keep);
    });
}

// A waypoint another module makes (Base Finder): replaces one of the same name. Waypoints shows it only if you have it on.
pub(super) fn add_waypoint(game: &Game<'_>, name: String, pos: [f64; 3]) {
    let dim = game.dim.id().to_owned();
    with(|s| {
        s.waypoints.retain(|w| w.name != name || w.dim != dim);
        s.waypoints.push(Waypoint { name, pos, dim });
    });
    save(game);
}

// Drops every waypoint a module made, those whose name `made` claims (Base Finder's "Base x z" when it turns off).
pub(super) fn remove_waypoints(game: &Game<'_>, made: impl Fn(&str) -> bool) {
    if with(|s| s.waypoints.iter().any(|w| made(&w.name))) {
        with(|s| s.waypoints.retain(|w| !made(&w.name)));
        save(game);
    }
}

// Death Location's waypoints: "Death", then "Death 2", "Death 3"... from the newest.
fn death_point(w: &Waypoint) -> bool {
    w.name == "Death" || w.name.strip_prefix("Death ").is_some_and(|n| n.parse::<u32>().is_ok())
}

pub(super) fn death_location(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.native_player() else {
        return;
    };
    let here = Waypoint { name: "Death".into(), pos: [p.x.floor() + 0.5, p.y.floor(), p.z.floor() + 0.5], dim: p.dimension.to_owned() };
    super::sunset_util::report(game, &format!("You died at {} {} {} in {}", here.pos[0].floor(), here.pos[1], here.pos[2].floor(), here.dim.trim_start_matches("minecraft:")));
    let keep = m.n("Keep").max(1.0) as usize;
    with(|s| {
        let mut deaths: Vec<Waypoint> = s.waypoints.iter().filter(|w| death_point(w)).cloned().collect();
        // Oldest first: Death n ... Death 2, Death.
        deaths.sort_by_key(|w| std::cmp::Reverse(w.name.strip_prefix("Death ").and_then(|n| n.parse::<u32>().ok()).unwrap_or(1)));
        deaths.push(here);
        let start = deaths.len().saturating_sub(keep);
        let total = deaths.len() - start;
        s.waypoints.retain(|w| !death_point(w));
        for (i, mut w) in deaths.into_iter().skip(start).enumerate() {
            let age = total - i;
            w.name = if age == 1 { "Death".into() } else { format!("Death {age}") };
            s.waypoints.push(w);
        }
        if let Some(i) = s.find("Waypoints") {
            s.modules[i].on = true;
        }
    });
    save(game);
}

// Clear on reach: a death waypoint within 3 blocks of you, in its dimension, is done with.
pub(super) fn reach_deaths(game: &mut Game<'_>) {
    let Some(p) = game.native_player() else {
        return;
    };
    let (at, dim) = ([p.x, p.y, p.z], p.dimension.to_owned());
    let gone: Vec<String> = with(|s| {
        let near = |w: &Waypoint| death_point(w) && w.dim == dim && (0..3).map(|a| (w.pos[a] - at[a]).powi(2)).sum::<f64>() <= 9.0;
        let gone = s.waypoints.iter().filter(|w| near(w)).map(|w| w.name.clone()).collect();
        s.waypoints.retain(|w| !near(w));
        gone
    });
    if !gone.is_empty() {
        save(game);
        super::sunset_util::report(game, &format!("Reached {}", gone.join(", ")));
    }
}

// Join and leave messages, from the server's player list.
pub(super) fn notify(game: &mut Game<'_>, m: &Module) {
    let Some(mut now) = game.remote_online_names() else {
        return;
    };
    now.sort();
    let Some(before) = with(|s| s.notify.replace(now.clone())) else {
        return;
    };
    for name in now.iter().filter(|n| !before.contains(n)) {
        if m.b("Join") {
            super::sunset_util::report(game, &format!("{name} joined"));
        }
    }
    for name in before.iter().filter(|n| !now.contains(n)) {
        if m.b("Leave") {
            super::sunset_util::report(game, &format!("{name} left"));
        }
    }
}

// Hole ESP (Sunset's): open blocks Min height tall, walled below and on the four sides by bedrock (unbreakable) or
// blast-proof blocks (obsidian and the like), or with Doubles two such side by side. Searched again every second, a
// few slices a tick.
pub(super) fn scan_holes(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(at) = block_pos(game) else {
        return;
    };
    let (h, v) = (m.n("Horizontal radius") as i32, m.n("Vertical radius") as i32);
    let (height, doubles, own, webs) = (m.n("Min height") as i32, m.b("Doubles"), m.b("Ignore own"), m.b("Webs"));
    let facts = game.res.facts();
    let solid = |p: [i32; 3]| {
        let state = game.world.block(p[0], p[1], p[2]);
        !facts.is_air(state) && facts.get(state).full_collision
    };
    let open = |p: [i32; 3]| {
        let state = game.world.block(p[0], p[1], p[2]);
        !(own && p == at) && (webs || !facts.get(state).name.ends_with(":cobweb")) && (0..height).all(|i| !solid([p[0], p[1] + i, p[2]]))
    };
    // 0 bedrock, 1 blast-proof, None anything else.
    let wall = |p: [i32; 3]| {
        let state = game.world.block(p[0], p[1], p[2]);
        let f = facts.get(state);
        if facts.is_air(state) {
            None
        } else if f.full_collision && f.hardness < 0.0 {
            Some(0u8)
        } else if f.explosion_resistance >= 600.0 && f.hardness >= 0.0 {
            Some(1u8)
        } else {
            None
        }
    };
    const SIDES: [[i32; 3]; 5] = [[0, -1, 0], [1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
    let plus = |p: [i32; 3], d: [i32; 3]| [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
    let key = (h as u64) | (v as u64) << 8 | (height as u64) << 16 | u64::from(doubles) << 24 | u64::from(own) << 25 | u64::from(webs) << 26;
    with(|s| {
        s.holes.tick(ticks, at, h, key, 20, 4, |c, x, out| {
            for z in c[2] - h..=c[2] + h {
                'cell: for y in c[1] - v..=c[1] + v {
                    let p = [x, y, z];
                    if !open(p) {
                        continue;
                    }
                    let (mut walls, mut kinds, mut pair) = (0, [0u8; 2], false);
                    for d in SIDES {
                        let q = plus(p, d);
                        if let Some(k) = wall(q) {
                            walls += 1;
                            kinds[usize::from(k)] += 1;
                        } else if d[1] == -1 || !doubles || pair || !open(q) {
                            continue 'cell;
                        } else {
                            // The other half of a double: walled all round but towards this one.
                            for e in SIDES.iter().filter(|e| **e != [-d[0], 0, -d[2]]) {
                                match wall(plus(q, *e)) {
                                    Some(k) => {
                                        walls += 1;
                                        kinds[usize::from(k)] += 1;
                                    }
                                    None => continue 'cell,
                                }
                            }
                            pair = true;
                        }
                    }
                    if walls == if pair { 8 } else { 5 } {
                        let kind = if kinds[1] == 0 { 0 } else if kinds[0] == 0 { 1 } else { 2 };
                        out.push((p, kind));
                    }
                }
            }
            (2 * h as usize + 1) * (2 * v as usize + 1) * 6
        })
    });
}
