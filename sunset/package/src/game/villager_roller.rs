// Villager Roller (after Lemon's Villager Roller, GPL-3.0, behaviour only, written anew): rerolls a novice
// librarian's book trade. It takes the lectern nearest you (5 blocks) and the villager nearest that lectern, then goes
// round: open the villager's trades and read the enchanted book on offer; one on Enchants (`name` for any level,
// `name 3` for that level or better) at Max price emeralds or less stops it (the trades opened again for you with
// Keep trades open). Sunset reads the trades from the server's packets, which wait while a screen is up, so the
// trades are closed as soon as they show and read the tick after. Otherwise
// it closes them, breaks the lectern (the villager drops the job: a villager never traded with does), waits for that,
// puts the lectern back from your hotbar and waits for the villager to take the job again (Profession wait). A
// librarian that has been traded with keeps its trades for good: Villager Roller says so and stops.

use super::sunset::{Category, Module, say};
use super::{Game, Screen};
use super::sunset_compat::SunsetGame as _;
use super::sunset_util::{self as util, short};
use horizon_protocol::packets::play::{self, Clientbound as C, Serverbound as S};
use std::sync::Mutex;

pub(super) const NAME: &str = "Villager Roller";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::World, "Rerolls a librarian's book until it sells one you want.")
            .text("Enchants", "mending,unbreaking 3,efficiency 5,protection 4,sharpness 5,silk touch,fortune 3,feather falling 4")
            .num("Max price", 40.0, 1.0, 64.0, 1.0)
            .num("Profession wait (s)", 20.0, 2.0, 60.0, 1.0)
            .num("Retry delay (s)", 1.5, 0.5, 10.0, 0.5)
            .bool("Rotate", true)
            .bool("Keep trades open", true)
            .bool("Verbose", false),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    // Opening the trades (the tick it asked, how many times).
    Open { asked: u64, tries: u32 },
    // Breaking the lectern: the ticks left until its stop goes.
    Break { left: u32 },
    // The lectern broken (since when), waiting for the villager to drop the job.
    Drop { since: u64 },
    // Putting the lectern back (not before `at`).
    Place { at: u64 },
    // Waiting for the villager to take the job (until when).
    Hire { until: u64 },
}

struct Roll {
    lectern: [i32; 3],
    villager: u32,
    step: Step,
    rolls: u32,
}

#[derive(Default)]
struct State {
    roll: Option<Roll>,
    // The trades the server sent since the last tick: each book's enchantments and price, the villager's xp, and the
    // trade screen's container.
    offers: Option<(Vec<(Vec<(String, u32)>, u32)>, i32, i32)>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

// The wanted list: (enchantment's short id, least level).
fn wanted(list: &str) -> Vec<(String, u32)> {
    list.split(',')
        .filter_map(|e| {
            let words: Vec<&str> = e.split_whitespace().collect();
            let (name, level) = match words.split_last() {
                Some((last, rest)) if !rest.is_empty() && last.parse::<u32>().is_ok() => (rest.join("_"), last.parse().unwrap_or(1)),
                _ => (words.join("_"), 1),
            };
            (!name.is_empty()).then(|| (name.to_ascii_lowercase(), level))
        })
        .collect()
}

// The first book on offer the list wants at `max` or less: its words for the chat.
fn pick(offers: &[(Vec<(String, u32)>, u32)], want: &[(String, u32)], max: u32) -> Option<String> {
    offers.iter().find_map(|(enchants, price)| {
        enchants
            .iter()
            .find(|(id, level)| *price <= max && want.iter().any(|(w, least)| w == id && level >= least))
            .map(|(id, level)| format!("{} {level} for {price} emeralds", id.replace('_', " ")))
    })
}

fn villager_job(game: &Game<'_>, id: u32) -> Option<String> {
    game.mobs.iter().find(|m| m.id == id && !m.removed).map(|m| short(&m.extra.profession.id()).to_owned())
}

fn ticks(m: &Module, name: &str) -> u64 {
    (m.n(name) * 20.0).round() as u64
}

// The ticks the lectern takes to break with what you hold: the server lets the stop through at 0.7 of the progress.
fn break_ticks(game: &Game<'_>, pos: [i32; 3]) -> u32 {
    let f = util::facts(game, pos);
    // At least the hand's speed (a held item the game has no speed for reads 0).
    let speed = game.res.items().mining_speed(game.held(), &f.name).max(1.0);
    let ground = game.player.as_ref().is_none_or(|p| p.ground.on_ground);
    let per_tick = if f.hardness <= 0.0 { 1.0 } else { speed / f.hardness / 30.0 / if ground { 1.0 } else { 5.0 } };
    if game.creative || per_tick >= 1.0 { 0 } else { ((0.7 / per_tick).ceil() as u32).clamp(1, 200) }
}

fn stop(game: &mut Game<'_>, why: &str) {
    say(game, &format!("Villager Roller: {why}"));
    st(|s| s.roll = None);
    util::toggle_off(game, NAME);
}

// The packets of the last tick: the trades.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    let C::MerchantOffers(p) = packet else {
        return;
    };
    if st(|s| s.roll.is_none()) {
        return;
    }
    let Some(r) = game.remote.as_ref() else {
        return;
    };
    let items = r.builtin.get("minecraft:item");
    let books: Vec<(Vec<(String, u32)>, u32)> = p
        .offers
        .iter()
        .filter(|o| items.and_then(|t| t.name(o.result.item)) == Some("minecraft:enchanted_book"))
        .map(|o| {
            let enchants = o
                .result
                .added
                .iter()
                .filter_map(|(_, c)| match c {
                    horizon_protocol::item::Component::Enchantments(list) => Some(list),
                    _ => None,
                })
                .flatten()
                .filter_map(|&(id, level)| Some((short(r.registries.name("minecraft:enchantment", id)?).to_owned(), level.max(0) as u32)))
                .collect();
            (enchants, (o.cost_a.count + o.special_price).max(1) as u32)
        })
        .collect();
    st(|s| s.offers = Some((books, p.xp, p.container)));
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    // The trades are a screen of their own: it goes on with one open.
    let Some(m) = m.filter(|_| game.remote.is_some() && game.player.is_some()) else {
        st(|s| *s = State::default());
        return;
    };
    let now = game.ticks;
    if st(|s| s.roll.is_none()) {
        let Some(eye) = game.eye_position() else { return };
        let Some(lectern) = nearest_lectern(game, eye) else {
            return stop(game, "no lectern within 5 blocks");
        };
        let at = util::center_of(lectern);
        let villager = game
            .mobs
            .iter()
            .filter(|v| v.kind.id == "minecraft:villager" && v.alive() && !v.removed)
            .map(|v| (util::dist(at, [v.position.x, v.position.y, v.position.z]), v.id))
            .filter(|(d, _)| *d <= 8.0)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, villager)) = villager else {
            return stop(game, "no villager within 8 blocks of the lectern");
        };
        let step = if villager_job(game, villager).as_deref() == Some("librarian") { Step::Open { asked: 0, tries: 0 } } else { Step::Hire { until: now + ticks(m, "Profession wait (s)") } };
        st(|s| s.roll = Some(Roll { lectern, villager, step, rolls: 0 }));
    }
    let Some((lectern, villager, step, rolls)) = st(|s| s.roll.as_ref().map(|r| (r.lectern, r.villager, r.step, r.rolls))) else {
        return;
    };
    // With the trades open the game keeps the villager aside: still a librarian.
    let trading = game.station.is_some() || game.screen != Screen::Playing;
    let job = villager_job(game, villager).or_else(|| trading.then(|| "librarian".to_owned()));
    let Some(job) = job else {
        let near = game.mobs.iter().filter(|v| v.kind.id == "minecraft:villager" && !v.removed).count();
        return stop(game, &format!("the villager is gone ({near} villagers about)"));
    };
    // Sunset reads the server's packets only with no screen up, so the trades are closed as soon as they show: their
    // offers come in the tick after.
    if game.station.is_some() {
        // Only the trades it opened; a screen you opened waits.
        if matches!(step, Step::Open { asked, .. } if asked > 0) {
            close(game, 0);
        }
        return;
    }
    let offers = st(|s| s.offers.take());
    let placed = short(&util::facts(game, lectern).name) == "lectern";
    let next = match step {
        Step::Open { asked, tries } => {
            if let Some((books, xp, _)) = offers {
                if m.b("Verbose") {
                    let line = format!("Villager Roller: {} book{} on offer", books.len(), if books.len() == 1 { "" } else { "s" });
                    super::sunset::chat_line(game, &line);
                }
                let found = pick(&books, &wanted(m.t("Enchants")), m.n("Max price") as u32);
                if let Some(found) = found {
                    // Keep trades open: open them again to buy it.
                    if m.b("Keep trades open") {
                        game.send(S::Interact(play::Interact { id: villager as i32, hand: 0, location: [0.0, 1.0, 0.0], secondary: false }));
                    }
                    game.play_at_feet("minecraft:block.note_block.pling");
                    return stop(game, &format!("found {found} after {rolls} roll{}", if rolls == 1 { "" } else { "s" }));
                }
                if xp > 0 {
                    return stop(game, "this librarian has been traded with: its trades never change");
                }
                Step::Break { left: break_ticks(game, lectern) + 1 }
            } else if asked == 0 || now >= asked + 40 {
                if tries >= 3 {
                    return stop(game, "the villager won't open its trades (too far?)");
                }
                game.send(S::Interact(play::Interact { id: villager as i32, hand: 0, location: [0.0, 1.0, 0.0], secondary: false }));
                game.swing_arm();
                Step::Open { asked: now, tries: tries + 1 }
            } else {
                step
            }
        }
        Step::Break { left } => {
            if !placed {
                Step::Drop { since: now }
            } else {
                let full = break_ticks(game, lectern) + 1;
                if left == full {
                    let seq = util::sequence(game);
                    game.send(S::PlayerAction(play::PlayerAction { action: 0, pos: lectern, direction: 1, sequence: seq }));
                    game.swing_arm();
                }
                if left <= 1 {
                    if full > 1 {
                        let seq = util::sequence(game);
                        game.send(S::PlayerAction(play::PlayerAction { action: 3, pos: lectern, direction: 1, sequence: seq }));
                    }
                    Step::Drop { since: now }
                } else {
                    Step::Break { left: left - 1 }
                }
            }
        }
        Step::Drop { since } => {
            if placed && now >= since + 20 {
                // The break was refused: again.
                Step::Break { left: break_ticks(game, lectern) + 1 }
            } else if !placed && job == "none" {
                Step::Place { at: now }
            } else if now >= since + ticks(m, "Profession wait (s)") {
                return stop(game, "the villager kept its job with the lectern gone (traded with, or another lectern near?)");
            } else {
                step
            }
        }
        Step::Place { at } => {
            if placed {
                Step::Hire { until: now + ticks(m, "Profession wait (s)") }
            } else if now >= at {
                let Some(slot) = util::find_hotbar(game, |n| n == "lectern") else {
                    return stop(game, "no lectern in the hotbar");
                };
                util::place_block(game, lectern, slot, m.b("Rotate"), true);
                Step::Place { at: now + ticks(m, "Retry delay (s)") }
            } else {
                step
            }
        }
        Step::Hire { until } => {
            if job == "librarian" {
                st(|s| s.offers = None);
                st(|s| {
                    if let Some(r) = s.roll.as_mut() {
                        r.rolls += 1;
                    }
                });
                Step::Open { asked: 0, tries: 0 }
            } else if !placed {
                Step::Place { at: now }
            } else if now >= until {
                // Not hired in time: break the lectern and put it back.
                Step::Break { left: break_ticks(game, lectern) + 1 }
            } else {
                step
            }
        }
    };
    if m.b("Verbose") && std::mem::discriminant(&next) != std::mem::discriminant(&step) {
        let what = match next {
            Step::Open { .. } => "opening the trades",
            Step::Break { left } => &format!("breaking the lectern ({left} ticks)"),
            Step::Drop { .. } => "waiting for the villager to drop the job",
            Step::Place { .. } => "putting the lectern back",
            Step::Hire { .. } => "waiting for the villager to take the job",
        };
        super::sunset::chat_line(game, &format!("Villager Roller: {what}"));
    }
    st(|s| {
        if let Some(r) = s.roll.as_mut() {
            r.step = next;
        }
    });
}

// Closes the trades as the game's own close does: the container, then back to playing.
fn close(game: &mut Game<'_>, _container: i32) {
    game.close_container();
    if game.screen != Screen::Playing {
        game.screen = Screen::Playing;
        game.capture(true);
    }
}

fn nearest_lectern(game: &Game<'_>, eye: [f64; 3]) -> Option<[i32; 3]> {
    let base = eye.map(|v| v.floor() as i32);
    let mut best: Option<(f64, [i32; 3])> = None;
    for x in -5..=5 {
        for y in -5..=5 {
            for z in -5..=5 {
                let p = [base[0] + x, base[1] + y, base[2] + z];
                let d = util::dist(eye, util::center_of(p));
                if d <= 5.0 && best.is_none_or(|b| d < b.0) && short(&util::facts(game, p).name) == "lectern" {
                    best = Some((d, p));
                }
            }
        }
    }
    best.map(|b| b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wanted_list_reads_names_and_levels() {
        let w = wanted("mending, unbreaking 3,silk touch, feather falling 4,");
        assert_eq!(w, vec![("mending".into(), 1), ("unbreaking".into(), 3), ("silk_touch".into(), 1), ("feather_falling".into(), 4)]);
    }

    #[test]
    fn pick_wants_the_level_and_price() {
        let want = wanted("unbreaking 3,mending");
        let offers = vec![(vec![("unbreaking".to_string(), 2)], 10), (vec![("mending".to_string(), 1)], 50)];
        assert_eq!(pick(&offers, &want, 40), None);
        assert_eq!(pick(&offers, &want, 64).as_deref(), Some("mending 1 for 50 emeralds"));
        let offers = vec![(vec![("unbreaking".to_string(), 3)], 12)];
        assert_eq!(pick(&offers, &want, 40).as_deref(), Some("unbreaking 3 for 12 emeralds"));
    }
}
