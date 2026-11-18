// HUD's Tab game modes: the tab list marks players in spectator and creative mode after their names.
// Sunset Presence: for a Sunset developer, an orange sun before the names of the players who share their presence and
// were confirmed on this server (sunset_presence fills SUNSET; empty for everyone else).
use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

pub static MARK: AtomicBool = AtomicBool::new(false);
// The setting the cached layout was built with: a change rebuilds it.
static BUILT: AtomicBool = AtomicBool::new(false);
// The players to mark as Sunset users, and a count of its changes (the cached layout is rebuilt on one).
static SUNSET: Mutex<Vec<u128>> = Mutex::new(Vec::new());
static SUNSET_GEN: AtomicU64 = AtomicU64::new(0);
static SUNSET_BUILT: AtomicU64 = AtomicU64::new(0);
const ICON: &str = "\u{2600} ";
const ICON_COLOR: u32 = 0xDB5000;

pub fn set_sunset(players: Vec<u128>) {
    let mut s = SUNSET.lock().unwrap_or_else(|e| e.into_inner());
    if *s != players {
        *s = players;
        SUNSET_GEN.fetch_add(1, Relaxed);
    }
}

fn is_sunset(uuid: u128) -> bool {
    SUNSET.lock().unwrap_or_else(|e| e.into_inner()).contains(&uuid)
}
thread_local! {
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

// The tag after a name in game mode `mode`, and its colour: spectators aqua, creative gold.
pub fn tag(mode: i32) -> Option<(&'static str, u32)> {
    match mode {
        horizon_client::tab_list::SPECTATOR => Some((" [S]", 0x55FFFF)),
        1 => Some((" [C]", 0xFFAA00)),
        _ => None,
    }
}

// The marked name: the game's own (team colours, italic spectators), then the tag. None while marking is off, for a
// mode with no tag, or inside the game's own call made here.
pub fn marked(p: &horizon_client::tab_list::TabPlayer, board: &horizon_client::scoreboard::Scoreboard) -> Option<horizon_client::text::Text> {
    if BUSY.with(Cell::get) {
        return None;
    }
    let tag = tag(p.game_mode).filter(|_| MARK.load(Relaxed));
    let icon = is_sunset(p.uuid);
    if tag.is_none() && !icon {
        return None;
    }
    BUSY.with(|b| b.set(true));
    let mut name = horizon_client::tab_list::name_for_display(p, board);
    BUSY.with(|b| b.set(false));
    if icon {
        name = horizon_client::text::Text::colored(ICON, ICON_COLOR).append(name);
    }
    Some(match tag {
        Some((tag, color)) => name.append(horizon_client::text::Text::colored(tag, color)),
        None => name,
    })
}

// Before a layout: true when marking (`running`: Sunset is on) changed since the cached one was built.
pub fn changed(running: bool) -> bool {
    let now = running && MARK.load(Relaxed);
    let built = SUNSET_GEN.load(Relaxed);
    let marks = BUILT.swap(now, Relaxed) != now;
    SUNSET_BUILT.swap(built, Relaxed) != built || marks
}

// The listed players with their marks put into their tab-list names (the name the game draws is then the marked
// one; the list's cache sees the change): the game's own name (its display name, else the team's formatting of the
// profile name), the Sunset sun before it and the game mode's tag after it, neither in a spectator's italics.
pub fn mark_players(players: &mut [horizon_client::tab_list::TabPlayer], board: &horizon_client::scoreboard::Scoreboard, running: bool) {
    let marks = running && MARK.load(Relaxed);
    for p in players.iter_mut() {
        let tag = tag(p.game_mode).filter(|_| marks);
        let icon = running && is_sunset(p.uuid);
        if tag.is_none() && !icon {
            continue;
        }
        let base = p.display_name.clone().unwrap_or_else(|| {
            let name = serde_json::Value::String(p.name.clone());
            match board.players_team(&p.name) {
                Some(team) => team.formatted_name_comp(&name),
                None => name,
            }
        });
        let mut extra = Vec::new();
        if icon {
            extra.push(serde_json::json!({"text": ICON, "color": format!("#{ICON_COLOR:06X}"), "italic": false}));
        }
        extra.push(base);
        if let Some((tag, color)) = tag {
            extra.push(serde_json::json!({"text": tag, "color": format!("#{color:06X}"), "italic": false}));
        }
        p.display_name = Some(serde_json::json!({"text": "", "extra": extra}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectators_and_creative_players_are_tagged() {
        assert_eq!(tag(super::super::SPECTATOR).map(|t| t.0), Some(" [S]"));
        assert_eq!(tag(1).map(|t| t.0), Some(" [C]"));
        assert_eq!(tag(0), None);
        assert_eq!(tag(2), None);
    }

    #[test]
    fn a_new_sunset_list_rebuilds_the_layout_once() {
        changed(true);
        set_sunset(vec![7]);
        assert!(is_sunset(7) && !is_sunset(8));
        assert!(changed(true));
        assert!(!changed(true));
        // The same list again changes nothing.
        set_sunset(vec![7]);
        assert!(!changed(true));
        set_sunset(Vec::new());
        assert!(changed(true));
        assert!(!is_sunset(7));
    }
}
