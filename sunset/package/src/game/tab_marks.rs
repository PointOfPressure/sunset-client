// HUD's Tab game modes (and Sunset Presence's sun) and Streamer Mode's names on a server: the marks go into the listed players' tab-list names,
// each tick, over the names the server gave (kept here, so a mark taken away or a new name from the server shows as
// it is). The tab list's own drawing can't be changed on this build.
use std::collections::HashMap;
use std::sync::Mutex;

// Single player's scoreboard as of the last tick, which the integrated server's tab-list players are marked against
// as the tab list asks for them (local_tab_players' hook, local below).
static LOCAL_BOARD: Mutex<Option<horizon_client::scoreboard::Scoreboard>> = Mutex::new(None);

// By player: the server's tab-list name, and the marked one put in its place.
static MARKED: Mutex<Option<HashMap<u128, (Option<serde_json::Value>, serde_json::Value)>>> = Mutex::new(None);

pub fn apply(game: &mut super::Game<'_>, running: bool) {
    let mut marked = MARKED.lock().unwrap_or_else(|e| e.into_inner());
    let marked = marked.get_or_insert_with(HashMap::new);
    let Some(r) = game.remote.as_mut() else {
        marked.clear();
        let mut local = LOCAL_BOARD.lock().unwrap_or_else(|e| e.into_inner());
        if !running {
            *local = None;
        } else if local.as_ref() != Some(&game.score.board) {
            *local = Some(game.score.board.clone());
        }
        return;
    };
    *LOCAL_BOARD.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let board = &game.score.board;
    for (uuid, p) in r.tab_players.iter_mut() {
        let server = match marked.get(uuid) {
            Some((server, ours)) if p.display_name.as_ref() == Some(ours) => server.clone(),
            _ => p.display_name.clone(),
        };
        let mut one = [p.clone()];
        one[0].display_name = server.clone();
        crate::game::sunset_tab_modes::mark_players(&mut one, board, running);
        let [mut shown] = one;
        // Streamer Mode: the names it hides, in the tab-list name too.
        if running {
            let mut name = shown.display_name.clone().unwrap_or_else(|| crate::game::sunset_tab_modes::base_comp(&shown, board));
            if crate::game::sunset_streamer::protect_json(&mut name) {
                shown.display_name = Some(name);
            }
        }
        match shown.display_name {
            Some(ours) if Some(&ours) != server.as_ref() => {
                p.display_name = Some(ours.clone());
                marked.insert(*uuid, (server, ours));
            }
            _ => {
                p.display_name = server;
                marked.remove(uuid);
            }
        }
    }
    marked.retain(|uuid, _| r.tab_players.contains_key(uuid));
}

// Single player's tab list: the integrated server's players with the same marks and Streamer Mode names a server's get.
pub fn local(mut players: Vec<horizon_client::tab_list::TabPlayer>) -> Vec<horizon_client::tab_list::TabPlayer> {
    let local = LOCAL_BOARD.lock().unwrap_or_else(|e| e.into_inner());
    let Some(board) = local.as_ref() else {
        return players;
    };
    crate::game::sunset_tab_modes::mark_players(&mut players, board, true);
    for p in players.iter_mut() {
        let mut name = p.display_name.clone().unwrap_or_else(|| crate::game::sunset_tab_modes::base_comp(p, board));
        if crate::game::sunset_streamer::protect_json(&mut name) {
            p.display_name = Some(name);
        }
    }
    players
}
