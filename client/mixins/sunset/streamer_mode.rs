// Streamer Mode (Client): your name and other players' names are replaced wherever the game draws text (chat, the tab
// list, the scoreboard, titles, boss bars, tooltips, name tags over heads, signs, every screen, Horizon's own menus and
// Sunset's HUD), as whole words in any case, also split over colours. Yours shows as Your name; others as Player 1,
// Player 2... (Numbered, the same number for a name while it is on) or all as Others' name. The names come from the
// server's player list (and the last word of a listed display name, a nickname), the players in the world, your
// profile and signed-in account, and Also hide. Nothing sent to the server changes: only what is drawn.

use super::Game;
use super::sunset::{Category, Module};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) const NAME: &str = "Streamer Mode";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Client, "Hides your name and other players' names on screen.")
            .bool("Hide your name", true)
            .text("Your name", "You")
            .bool("Hide others", true)
            .pick("Others as", &["Numbered", "Fixed"])
            .text("Others' name", "Player")
            .text("Also hide", ""),
    ]
}

#[derive(Default)]
struct State {
    // The number each other name was given while the module is on: every name seen stays hidden (a player who left is
    // still named in chat), with the same number.
    numbers: HashMap<String, usize>,
    // The last list handed to the text hooks, when it was made, and the players it was made from.
    shown: Option<HashMap<String, String>>,
    at: Option<Instant>,
    players: u128,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

// A name Minecraft allows: 1 to 16 letters, digits or _.
fn is_name(s: &str) -> bool {
    (1..=16).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// The name as text shows it: a Bedrock player's prefix (Geyser's ".Steve") dropped, as it is no part of the word.
fn bare(s: &str) -> &str {
    s.trim_start_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
}

// A replacement shown in place of a name: one line, at most 32 characters.
fn shown_as(s: &str, fallback: &str) -> String {
    let s: String = s.trim().chars().filter(|c| !c.is_control()).take(32).collect();
    if s.is_empty() { fallback.to_owned() } else { s }
}

// Every frame, also on the title screen: the names to hide, made again at once when a player joins, leaves or comes
// into the world (before the frame draws their name), and four times a second for settings and nicknames.
pub(super) fn frame(game: &Game<'_>) {
    let module = super::sunset::with(|s| s.module(NAME).filter(|m| m.on).cloned()).filter(|_| game.native_on(super::sunset::ID));
    let Some(m) = module else {
        if state(|s| s.shown.take().is_some()) {
            horizon_client_render::hud::sunset_streamer::set(None);
        }
        state(|s| s.numbers.clear());
        return;
    };
    let players = super::remote::sunset_remote::players_changed_key(game);
    let due = state(|s| {
        s.shown.is_none() || s.players != players || s.at.is_none_or(|t| t.elapsed() >= Duration::from_millis(250))
    });
    if !due {
        return;
    }
    let (own, others) = super::remote::sunset_remote::player_names(game);
    let account = horizon_client::auth::current().map(|a| a.name.clone());
    let extra: Vec<String> = m.t("Also hide").split(',').map(|s| s.trim().to_owned()).collect();
    let names = build(&m, own.into_iter().chain(account), others.into_iter().chain(extra));
    state(|s| {
        s.at = Some(Instant::now());
        s.players = players;
        if s.shown.as_ref() != Some(&names) {
            horizon_client_render::hud::sunset_streamer::set(Some(names.clone()));
            s.shown = Some(names);
        }
    });
}

// Each name to hide (lower case) and what shows in its place. Your names win over a player with the same one.
fn build(m: &Module, own: impl Iterator<Item = String>, others: impl Iterator<Item = String>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if m.b("Hide others") {
        let fixed = shown_as(m.t("Others' name"), "Player");
        let numbered = m.p("Others as") == "Numbered";
        state(|s| {
            for name in others {
                let name = bare(&name);
                if !is_name(name) {
                    continue;
                }
                let next = s.numbers.len() + 1;
                s.numbers.entry(name.to_ascii_lowercase()).or_insert(next);
            }
            for (key, n) in &s.numbers {
                out.insert(key.clone(), if numbered { format!("{fixed} {n}") } else { fixed.clone() });
            }
        });
    }
    if m.b("Hide your name") {
        let yours = shown_as(m.t("Your name"), "You");
        for name in own {
            let name = bare(&name);
            if is_name(name) {
                out.insert(name.to_ascii_lowercase(), yours.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module() -> Module {
        defaults().remove(0)
    }

    #[test]
    fn yours_and_others_are_replaced_and_numbers_stay() {
        let m = module();
        let names = build(&m, ["vstxx".to_owned()].into_iter(), ["Notch".into(), "jeb_".into(), "bad name".into()].into_iter());
        assert_eq!(names.get("vstxx").map(String::as_str), Some("You"));
        let notch = names.get("notch").cloned().expect("listed");
        assert!(notch.starts_with("Player "));
        assert!(!names.contains_key("bad name"), "not a Minecraft name");
        let again = build(&m, std::iter::empty(), ["Notch".into(), ".Steve".into()].into_iter());
        assert_eq!(again.get("notch"), Some(&notch), "the same number while on");
        assert!(again.contains_key("jeb_"), "a player who left stays hidden");
        assert!(again.contains_key("steve"), "a Bedrock player's prefix is dropped");
    }

    #[test]
    fn replacements_are_one_short_line() {
        assert_eq!(shown_as("  ", "You"), "You");
        assert_eq!(shown_as("a\nb", "You"), "ab");
        assert_eq!(shown_as(&"x".repeat(40), "You").len(), 32);
        assert!(is_name("jeb_") && !is_name("a b") && !is_name(&"a".repeat(17)));
    }
}
