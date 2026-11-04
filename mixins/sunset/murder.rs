// ESP's Murderer finder: every other player seen with one of the Murderer items in hand is
// remembered until the world changes, drawn in the Murderer color, listed at the top of the screen and, with Callout,
// named in chat, one line a delay.
use super::sunset::{Module, Shape};
use super::{Dim, Game};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Finder {
    world: Option<(i32, Dim)>,
    suspects: Vec<(u32, String)>,
    pending: VecDeque<String>,
    sent: Option<Instant>,
}

static FINDER: Mutex<Option<Finder>> = Mutex::new(None);

fn finder<R>(f: impl FnOnce(&mut Finder) -> R) -> R {
    f(FINDER.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Finder::default))
}

// An item on the list: by its id, or by a tag of the game's (swords, axes, pickaxes, shovels).
fn listed(game: &Game<'_>, list: &str, item: &str) -> bool {
    let short = super::sunset_util::short(item);
    list.split(',').map(str::trim).filter(|e| !e.is_empty()).any(|e| {
        let e = super::sunset_util::short(e);
        e == short || game.res.items().item_tags.get(&format!("minecraft:{e}")).is_some_and(|items| items.contains(item))
    })
}

// The callout for `name`: one of the messages (split by ;), picked by the tick, with %s as the name.
fn callout(m: &Module, name: &str, tick: u64) -> Option<String> {
    let messages: Vec<&str> = m.t("Callout messages").split(';').map(str::trim).filter(|s| !s.is_empty()).collect();
    let pick = messages.get((tick.wrapping_mul(0x9E37_79B9) % messages.len().max(1) as u64) as usize)?;
    Some(pick.replace("%s", name))
}

pub(super) fn tick(game: &mut Game<'_>, esp: Option<&Module>) {
    let Some(m) = esp.filter(|m| m.b("Murderer finder")) else {
        finder(|f| *f = Finder::default());
        return;
    };
    let world = game.remote.as_ref().map(|r| (r.entity_id, game.dim));
    let own = game.remote.as_ref().map(|r| r.entity_id);
    finder(|f| {
        if f.world != world {
            *f = Finder { world, ..Finder::default() };
        }
    });
    if world.is_none() {
        return;
    }
    let delay = Duration::from_millis(m.n("Callout delay (ms)").max(0.0) as u64);
    if let Some(line) = finder(|f| {
        (!f.pending.is_empty() && f.sent.is_none_or(|t| t.elapsed() >= delay)).then(|| {
            f.sent = Some(Instant::now());
            f.pending.pop_front()
        })
    })
    .flatten()
    {
        game.send_chat(&line);
    }
    let names = game.remote_player_names();
    let found: Vec<(u32, String)> = game
        .mobs
        .iter()
        .filter(|mob| mob.kind.id == "minecraft:player" && mob.alive() && !mob.removed && Some(mob.id as i32) != own)
        .filter(|mob| mob.extra.main_hand.as_deref().is_some_and(|item| listed(game, m.t("Murderer items"), item)))
        .filter(|mob| finder(|f| f.suspects.iter().all(|s| s.0 != mob.id)))
        .map(|mob| (mob.id, names.iter().find(|n| n.0 == mob.id).map_or_else(|| format!("#{}", mob.id), |n| n.1.clone())))
        .collect();
    for (id, name) in found {
        let line = m.b("Callout").then(|| callout(m, &name, game.ticks)).flatten();
        finder(|f| {
            f.suspects.push((id, name));
            f.pending.extend(line);
        });
    }
}

// A player Murderer finder has seen with a murderer item.
pub(super) fn suspect(id: u32) -> bool {
    finder(|f| f.suspects.iter().any(|s| s.0 == id))
}

// Murderer list: the suspects still in the world, under its title at the top middle of the screen.
pub(super) fn draw(game: &Game<'_>, esp: &Module, w: f32, out: &mut Vec<Shape>) {
    if !esp.b("Murderer finder") || !esp.b("Murderer list") {
        return;
    }
    let here: Vec<String> = finder(|f| {
        f.suspects.iter().filter(|s| game.mobs.iter().any(|m| m.id == s.0 && !m.removed)).map(|s| s.1.clone()).collect()
    });
    if here.is_empty() {
        return;
    }
    let colour = esp.c("Murderer color") & 0xFF_FFFF;
    let title = "Murderer list";
    out.push(Shape::Text(title.to_owned(), [(w - game.native_text_width(title)) / 2.0, 15.0], 0xFFFFFF));
    for (row, name) in here.into_iter().enumerate() {
        out.push(Shape::Text(name.clone(), [(w - game.native_text_width(&name)) / 2.0, 26.0 + row as f32 * 10.0], colour));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_callout_names_the_player_in_one_of_the_messages() {
        let m = Module::new("ESP", super::super::sunset::Category::Render, "")
            .text("Callout messages", "%s is the murderer!; i saw that %s!");
        let said: std::collections::BTreeSet<String> = (0..64).filter_map(|t| callout(&m, "Alex", t)).collect();
        assert_eq!(said.into_iter().collect::<Vec<_>>(), ["Alex is the murderer!", "i saw that Alex!"]);
        let none = Module::new("ESP", super::super::sunset::Category::Render, "").text("Callout messages", " ; ");
        assert_eq!(callout(&none, "Alex", 3), None);
    }
}
