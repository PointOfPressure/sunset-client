// HUD's Tab game modes (and Sunset Presence's sun) on a server: the marks go into the listed players' tab-list names,
// each tick, over the names the server gave (kept here, so a mark taken away or a new name from the server shows as
// it is). The tab list's own drawing can't be changed on this build.
use std::collections::HashMap;
use std::sync::Mutex;

// By player: the server's tab-list name, and the marked one put in its place.
static MARKED: Mutex<Option<HashMap<u128, (Option<serde_json::Value>, serde_json::Value)>>> = Mutex::new(None);

pub fn apply(game: &mut super::Game<'_>, running: bool) {
    let mut marked = MARKED.lock().unwrap_or_else(|e| e.into_inner());
    let marked = marked.get_or_insert_with(HashMap::new);
    let Some(r) = game.remote.as_mut() else {
        marked.clear();
        return;
    };
    let board = &game.score.board;
    for (uuid, p) in r.tab_players.iter_mut() {
        let server = match marked.get(uuid) {
            Some((server, ours)) if p.display_name.as_ref() == Some(ours) => server.clone(),
            _ => p.display_name.clone(),
        };
        let mut one = [p.clone()];
        one[0].display_name = server.clone();
        crate::game::sunset_tab_modes::mark_players(&mut one, board, running);
        let [shown] = one;
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
