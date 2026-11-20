// Sign Historian (after Path-Seeker's Sign Historian, behaviour only, written anew): remembers every sign's text where
// it stands, for each world or server, kept between sessions in Sunset's data folder (Max signs, the oldest dropped). Coming back, a sign that
// is gone (its chunk loaded, no sign there) is boxed in Gone color and one whose text changed, or whose wax came off,
// in Changed color, for the rest of the session, and said once in the chat with what it said before; from then on
// what is there now is remembered. Signs you broke or wrote yourself are only remembered again.

use super::Game;
use super::sunset::{Camera, Canvas, Category, Module, say};
use super::sunset_compat::SunsetGame as _;
use super::sunset_util::{self as util, short};
use super::sunset_visual::{Look, draw_box};
use horizon_client::containers::BlockEntity;
use horizon_protocol::packets::play::Serverbound as S;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub(super) const NAME: &str = "Sign Historian";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Render, "Remembers signs and shows those gone or changed since.")
            .bool("Gone", true)
            .bool("Changed", true)
            .bool("Chat", true)
            .bool("Labels", true)
            .colour("Gone color", "Red")
            .colour("Changed color", "Yellow")
            .num("Max signs", 500.0, 50.0, 5000.0, 50.0),
    ]
}

// A sign as remembered: both sides' lines (joined by new lines) and its wax.
#[derive(Clone, Debug, PartialEq)]
struct Sign {
    front: String,
    back: String,
    waxed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    Gone,
    Changed,
    Unwaxed,
}

#[derive(Default)]
struct State {
    // The world's key; the signs remembered in the order first seen, and where each is.
    key: String,
    order: Vec<[i32; 3]>,
    signs: HashMap<[i32; 3], Sign>,
    // What changed since last time (and what it said), the changes said, and your own breaks and edits.
    changes: HashMap<[i32; 3], (Change, String)>,
    said: HashSet<[i32; 3]>,
    mine: HashSet<[i32; 3]>,
    dirty: bool,
    saved: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

fn escape(s: &str) -> String {
    s.replace('%', "%25").replace('|', "%7C").replace(';', "%3B").replace('\n', "%0A")
}

fn unescape(s: &str) -> String {
    s.replace("%0A", "\n").replace("%3B", ";").replace("%7C", "|").replace("%25", "%")
}

fn encode(order: &[[i32; 3]], signs: &HashMap<[i32; 3], Sign>) -> String {
    let rows: Vec<String> = order
        .iter()
        .filter_map(|p| {
            let s = signs.get(p)?;
            Some(format!("{},{},{},{}|{}|{}", p[0], p[1], p[2], u8::from(s.waxed), escape(&s.front), escape(&s.back)))
        })
        .collect();
    rows.join(";")
}

fn decode(text: &str) -> Vec<([i32; 3], Sign)> {
    text.split(';')
        .filter_map(|row| {
            let mut parts = row.split('|');
            let mut n = parts.next()?.split(',').map(|v| v.trim().parse::<i32>().ok());
            let p = [n.next()??, n.next()??, n.next()??];
            let waxed = n.next()?? == 1;
            Some((p, Sign { front: unescape(parts.next()?), back: unescape(parts.next()?), waxed }))
        })
        .collect()
}

fn sign_of(game: &Game<'_>, p: [i32; 3]) -> Option<Sign> {
    match game.world.block_entities.get(&(p[0], p[1], p[2])) {
        Some(BlockEntity::Sign(s)) => Some(Sign { front: s.front.lines.join("\n"), back: s.back.lines.join("\n"), waxed: s.waxed }),
        _ => None,
    }
}

// What a remembered sign said, short, for the chat and labels.
fn words(s: &Sign) -> String {
    let all: Vec<&str> = s.front.split('\n').chain(s.back.split('\n')).map(str::trim).filter(|l| !l.is_empty()).collect();
    let text = all.join(" / ");
    if text.is_empty() { "nothing".into() } else if text.chars().count() > 40 { format!("{}...", text.chars().take(40).collect::<String>()) } else { text }
}

// What changed between the remembered sign and the one there now (None: the same).
fn compare(was: &Sign, now: Option<&Sign>) -> Option<Change> {
    match now {
        None => Some(Change::Gone),
        Some(n) if n.front != was.front || n.back != was.back => Some(Change::Changed),
        Some(n) if was.waxed && !n.waxed => Some(Change::Unwaxed),
        _ => None,
    }
}

// Your own sign work: a break started or a sign edited there is yours.
pub(super) fn sent(packet: &S) {
    let pos = match packet {
        S::PlayerAction(p) if p.action == 0 || p.action == 3 => p.pos,
        S::SignUpdate(p) => p.pos,
        _ => return,
    };
    if STATE.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        st(|s| {
            if s.signs.contains_key(&pos) {
                s.mine.insert(pos);
            }
        });
    }
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m.filter(|_| util::in_world(game)) else {
        if m.is_none() {
            save(game);
            *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        return;
    };
    let key = format!("sign-history-{:016x}-{}.txt", util::world_hash(game).unwrap_or(0), game.dim.id().replace(':', "_"));
    if st(|s| s.key != key) {
        save(game);
        let text = std::fs::read_to_string(game.native_data_dir(super::sunset::ID).join(&key)).unwrap_or_default();
        let loaded = decode(&text);
        st(|s| {
            *s = State { key: key.clone(), ..State::default() };
            for (p, sign) in loaded {
                s.order.push(p);
                s.signs.insert(p, sign);
            }
        });
    }
    if game.ticks % 10 != 0 {
        return;
    }
    // Signs now: (where, what), and the remembered places in loaded chunks.
    let now: Vec<([i32; 3], Sign)> = game
        .world
        .block_entities
        .iter()
        .filter_map(|(&(x, y, z), e)| matches!(e, BlockEntity::Sign(_)).then(|| [x, y, z]))
        .filter_map(|p| Some((p, sign_of(game, p)?)))
        .collect();
    let known: Vec<[i32; 3]> = st(|s| s.order.clone());
    let loaded: Vec<([i32; 3], Option<Sign>)> = known
        .into_iter()
        .filter(|p| game.world.chunks.contains_key(&(p[0] >> 4, p[2] >> 4)))
        .map(|p| {
            // A sign block with no block entity yet: wait for it.
            let block = short(&util::facts(game, p).name).ends_with("sign");
            (p, sign_of(game, p).or_else(|| block.then(|| Sign { front: "\u{0}".into(), back: String::new(), waxed: false })))
        })
        .collect();
    let max = m.n("Max signs") as usize;
    let fresh = st(|s| {
        let mut fresh: Vec<([i32; 3], Change, String)> = Vec::new();
        for (p, there) in loaded {
            if there.as_ref().is_some_and(|t| t.front == "\u{0}") {
                continue;
            }
            let Some(was) = s.signs.get(&p) else { continue };
            if s.mine.contains(&p) {
                // Your own: remember what is there now (or forget it).
                match there {
                    Some(t) => {
                        s.signs.insert(p, t);
                    }
                    None => {
                        s.signs.remove(&p);
                        s.order.retain(|o| *o != p);
                    }
                }
                s.mine.remove(&p);
                s.changes.remove(&p);
                s.dirty = true;
                continue;
            }
            // A change is said once and marked for the session; what is there now is remembered from then on.
            if let Some(c) = compare(was, there.as_ref()) {
                let before = words(was);
                if s.said.insert(p) {
                    fresh.push((p, c, before.clone()));
                }
                s.changes.insert(p, (c, before));
                match there {
                    Some(t) => {
                        s.signs.insert(p, t);
                    }
                    None => {
                        s.signs.remove(&p);
                        s.order.retain(|o| *o != p);
                    }
                }
                s.dirty = true;
            }
        }
        // New signs are remembered.
        for (p, sign) in now {
            if !s.signs.contains_key(&p) {
                s.order.push(p);
                s.signs.insert(p, sign);
                s.dirty = true;
            }
        }
        while s.order.len() > max {
            let p = s.order.remove(0);
            s.signs.remove(&p);
            s.changes.remove(&p);
            s.dirty = true;
        }
        fresh
    });
    for (p, c, before) in fresh {
        let shown = match c {
            Change::Gone => m.b("Gone"),
            Change::Changed | Change::Unwaxed => m.b("Changed"),
        };
        if shown && m.b("Chat") {
            let what = match c {
                Change::Gone => "gone",
                Change::Changed => "changed",
                Change::Unwaxed => "unwaxed",
            };
            say(game, &format!("Sign Historian: sign at {} {} {} {what} (said: {before})", p[0], p[1], p[2]));
        }
    }
    if st(|s| s.dirty && game.ticks >= s.saved + 200) {
        save(game);
    }
}

fn save(game: &Game<'_>) {
    let out = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut().filter(|s| s.dirty && !s.key.is_empty()).map(|s| {
        s.dirty = false;
        s.saved = game.ticks;
        (s.key.clone(), encode(&s.order, &s.signs))
    });
    if let Some((key, text)) = out {
        let dir = game.native_data_dir(super::sunset::ID);
        let _ = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(key), text));
    }
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else { return };
    let marks: Vec<([i32; 3], Change, String)> =
        STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.changes.iter().map(|(p, (c, w))| (*p, *c, w.clone())).collect()).unwrap_or_default();
    for (p, c, before) in marks {
        let (on, argb) = match c {
            Change::Gone => (m.b("Gone"), m.c("Gone color")),
            Change::Changed | Change::Unwaxed => (m.b("Changed"), m.c("Changed color")),
        };
        if !on {
            continue;
        }
        let lo = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
        draw_box(cam, out, &Look::BOXED, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], argb, lw);
        if m.b("Labels") && let Some([sx, sy]) = cam.project([lo[0] + 0.5, lo[1] + 1.3, lo[2] + 0.5]) {
            out.text(&before, [sx - game.sunset_text_width(&before) / 2.0, sy], argb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_survive_saving() {
        let mut signs = HashMap::new();
        let p = [1, -2, 3];
        signs.insert(p, Sign { front: "a|b;c%d\nsecond".into(), back: String::new(), waxed: true });
        let back = decode(&encode(&[p], &signs));
        assert_eq!(back, vec![(p, signs[&p].clone())]);
    }

    #[test]
    fn changes_are_gone_changed_or_unwaxed() {
        let was = Sign { front: "hi".into(), back: String::new(), waxed: true };
        assert_eq!(compare(&was, None), Some(Change::Gone));
        assert_eq!(compare(&was, Some(&Sign { front: "bye".into(), ..was.clone() })), Some(Change::Changed));
        assert_eq!(compare(&was, Some(&Sign { waxed: false, ..was.clone() })), Some(Change::Unwaxed));
        assert_eq!(compare(&was, Some(&was)), None);
    }
}
