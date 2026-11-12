// Things Sunset needs from the connection's private state.
use crate::game::Game;

// The game's own sequence counter (the server wants block-action sequence
// numbers that never go down), for Sunset's placing and breaking.
pub(crate) fn next(game: &mut Game<'_>) -> i32 {
    game.next_sequence()
}

// A right click for the tick's keybind pass (Minecraft's keyUse.consumeClick): it waits for no use delay, and the
// game places with its own sequence number and prediction, before the tick's movement. Scaffold's Godbridge.
pub(crate) fn press_use(game: &mut Game<'_>) {
    super::sunset_combat_remote::click(game, false);
}

// Where the game's own right click on `hit` with the stack in hotbar `slot` would put a block, as the game predicts it
// (the block item's placement context: the clicked block itself when it can be replaced, else the one past the face,
// and nothing when the block would not go in), or None. Dry: the world is not changed. Scaffold's Normal checks each
// click with it before pressing.
pub(crate) fn places_into(game: &mut Game<'_>, hit: &crate::game::Hit, slot: usize) -> Option<[i32; 3]> {
    let stack = game.stack_in(slot)?.clone();
    game.remote_can_place(hit, &stack).map(|(pos, _)| [pos.x, pos.y, pos.z])
}

// Telly clicks immediately before the movement pass. The native use owns sequencing, inventory consumption,
// interaction callbacks, prediction and the swing packet. Only the optional local arm animation is restored.
pub(crate) fn use_block(game: &mut Game<'_>, hit: crate::game::Hit, look: (f32, f32), swing: bool) {
    let previous_hit = game.forced_hit.replace(hit);
    let animation = game.swing;
    let view = (game.yaw, game.pitch);
    (game.yaw, game.pitch) = look;
    game.remote_use();
    (game.yaw, game.pitch) = view;
    game.forced_hit = previous_hit;
    if !swing {
        game.swing = animation;
    }
}

// A missing predicted block is only a rejection once the native prediction handler has settled it.
pub(crate) fn prediction_pending(game: &Game<'_>, cell: [i32; 3]) -> bool {
    game.remote
        .as_ref()
        .is_some_and(|r| r.predictions.contains_key(&cell))
}

// The held slot as the server knows it: true when `slot` is news to it (the caller sends the SetCarriedItem). The
// game sends one before anything the selected slot acts with whenever the selection differs from what it last told
// (`remote_send_carried`, MultiPlayerGameMode.ensureHasSentCarriedItem), so this keeps it from sending the same slot a
// second time (Grim BadPacketsA, Intave "sent slot twice"). False without a connection.
pub(crate) fn tell_carried(game: &mut Game<'_>, slot: usize) -> bool {
    game.remote.as_mut().is_some_and(|r| r.carried.replace(Some(slot)) != Some(slot))
}

// The wait the game puts between one block's breaking and the next (5 ticks).
pub(crate) fn clear_destroy_delay(game: &mut Game<'_>, wait: u32) {
    if let Some(r) = game.remote.as_mut() {
        r.destroy_delay = r.destroy_delay.min(wait);
    }
}

// The block being broken by hand and how far it is (0 to 1), for Speed Mine.
pub(crate) fn breaking(game: &Game<'_>) -> Option<([i32; 3], f32)> {
    game.remote.as_ref()?.destroying.map(|(pos, _, done)| (pos, done))
}

pub(crate) fn set_breaking(game: &mut Game<'_>, done: f32) {
    if let Some((_, _, d)) = game.remote.as_mut().and_then(|r| r.destroying.as_mut()) {
        *d = done;
    }
}

// No Break Reset: the item in hand is no longer the one the break began with (the game would start it again).
pub(crate) fn breaking_item_changed(game: &Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|r| r.destroying.is_some() && r.destroying_item != game.main_hand_key())
}

// ...so the break goes on with the new one.
pub(crate) fn keep_breaking(game: &mut Game<'_>) {
    let held = game.main_hand_key();
    if let Some(r) = game.remote.as_mut() {
        r.destroying_item = held;
    }
}

// Anti Vanish: the names in the tab list (the listed players, not you).
// Anti Vanish: everyone else the server's player info names, with their game mode and whether the tab list shows them.
pub(crate) fn tab_presence(game: &Game<'_>) -> Vec<(String, i32, bool)> {
    game.remote.as_ref().map_or_else(Vec::new, |r| {
        r.tab_players.values().filter(|p| p.uuid != r.profile).map(|p| (p.name.clone(), p.game_mode, r.listed.contains(&p.uuid))).collect()
    })
}

pub(crate) fn tab_names(game: &Game<'_>) -> Vec<String> {
    game.remote.as_ref().map_or_else(Vec::new, |r| {
        r.tab_players.values().filter(|p| p.uuid != r.profile && r.listed.contains(&p.uuid)).map(|p| p.name.clone()).collect()
    })
}

// Streamer Mode: your names (your profile's, and the last word of your listed display name, a nickname, when it is
// not your profile name) and everyone else's the same way from the server's player list, with the players in the world.
pub(crate) fn player_names(game: &Game<'_>) -> (Vec<String>, Vec<String>) {
    let Some(r) = game.remote.as_ref() else {
        return (Vec::new(), Vec::new());
    };
    let mut own = vec![r.profile_name.clone()];
    let mut others = Vec::new();
    for t in r.tab_players.values() {
        let list = if t.uuid == r.profile { &mut own } else { &mut others };
        list.push(t.name.clone());
        let shown = t.display_name.as_ref().map(horizon_client::scoreboard::comp_text).map(|text| text.plain());
        // A display name that shows the profile name is no nickname ("[VIP] Notch", "Notch [AFK]").
        let nick = shown.as_deref().and_then(|plain| {
            let words: Vec<&str> = plain.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect();
            (!words.iter().any(|w| w.eq_ignore_ascii_case(&t.name))).then(|| words.last().copied()).flatten()
        });
        if let Some(nick) = nick {
            list.push(nick.to_owned());
        }
    }
    others.extend(r.level_player_names.values().cloned());
    (own, others)
}

// Streamer Mode: what changes when a player joins, leaves or appears in the world (cheap enough for every frame).
pub(crate) fn players_changed_key(game: &Game<'_>) -> u128 {
    game.remote.as_ref().map_or(0, |r| {
        let listed = r.tab_players.keys().fold(0u128, |a, k| a ^ k.rotate_left(17));
        let level = r.level_player_names.keys().fold(0u128, |a, &k| a.wrapping_add(k as u128 * 0x9e37_79b9));
        listed ^ level ^ ((r.tab_players.len() as u128) << 96) ^ ((r.level_player_names.len() as u128) << 64)
    })
}
