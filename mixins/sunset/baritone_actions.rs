// Independently authored Horizon input mapping. No packets or engine algorithms live here.
use super::{Game, sunset_movement_owner as owner};
use horizon_client::held::Source;
use serde::Deserialize;
use std::sync::Mutex;
use winit::{event::MouseButton, keyboard::KeyCode};

#[derive(Clone, Debug, Deserialize)]
pub(super) struct BlockAction { pub pos: [i32; 3], pub state_id: u32 }
#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct Intent {
    pub movement: u8,
    pub yaw: Option<f32>, pub pitch: Option<f32>, pub selected_slot: Option<usize>,
    pub attack: Option<BlockAction>,
    #[serde(rename = "use")]
    pub use_block: Option<BlockAction>,
}
#[derive(Clone, Copy)]
struct Held {
    lease: Option<owner::Lease>, buttons: [bool; 2], clicks: [u32; 3],
    look: Option<((f32, f32), (f32, f32))>, slot: Option<(usize, usize)>,
}
static HELD: Mutex<Held> = Mutex::new(Held { lease: None, buttons: [false; 2], clicks: [0; 3], look: None, slot: None });

pub(super) fn movement(bits: u8) -> Result<Vec<KeyCode>, String> {
    if bits & !127 != 0 { return Err("Unknown movement flags.".into()); }
    let bits = if bits & 3 == 3 { bits & !3 } else { bits };
    let bits = if bits & 12 == 12 { bits & !12 } else { bits };
    Ok(owner::MOVE_KEYS.iter().enumerate().filter_map(|(i, &k)| (bits & (1 << i) != 0).then_some(k)).collect())
}

pub(super) fn validate(intent: &Intent) -> Result<(), String> {
    movement(intent.movement)?;
    if intent.yaw.is_some_and(|v| !v.is_finite() || !(-180.0..180.0).contains(&v))
        || intent.pitch.is_some_and(|v| !v.is_finite() || !(-90.0..=90.0).contains(&v))
        || intent.selected_slot.is_some_and(|s| s > 8)
        || intent.attack.is_some() && intent.use_block.is_some() {
        return Err("Invalid Baritone aim, slot or simultaneous clicks.".into());
    }
    Ok(())
}

fn prepare_sprint<E>(history: &mut horizon_client::sprint::SprintState, wanted: bool, current: bool,
    set: impl FnOnce(bool) -> Result<(), E>) -> Result<(), E> {
    if !wanted {
        history.trigger_time = 0;
        if current { set(false)?; }
    }
    Ok(())
}

fn sprint_state(game: &mut Game<'_>, wanted: bool) -> Result<(), String> {
    if let Some(p) = game.player.as_mut() {
        let mut history = p.sprint_input;
        let result = prepare_sprint(&mut history, wanted, p.sprinting, |sprinting| p.set_sprinting(sprinting));
        p.sprint_input = history;
        result.map_err(|e| format!("Could not update native sprint: {e}"))?;
    }
    Ok(())
}

// A mapped engine hold must not become a native toggle. Keep the native KeyState separate from the mapped
// attack/use flag, so a physical toggle press can take over and a screen cannot remember our synthetic hold.
fn button(game: &mut Game<'_>, i: usize, down: bool) {
    if !down {
        if super::agent::sunset_goto::foreign_input(game).1[i] { return; }
        let native = super::sunset::mapped_input(&game.settings.keys, &game.physical_keys, &game.physical_buttons, [None; 4]);
        if native.1.contains(&if i == 0 { MouseButton::Left } else { MouseButton::Right })
            || game.toggles.is_down(i + 2) { return; }
    }
    let options = [game.settings.toggle_attack, game.settings.toggle_use];
    let native = game.toggles.0[i + 2].clone();
    game.settings.toggle_attack = false;
    game.settings.toggle_use = false;
    let _ = game.mouse_mapping(if i == 0 { MouseButton::Left } else { MouseButton::Right }, down, Source::Agent);
    game.settings.toggle_attack = options[0];
    game.settings.toggle_use = options[1];
    game.toggles.0[i + 2] = native;
    // Sleeping can make play_mouse return before updating its mapping; cancellation still releases our flag.
    if !down && !game.input_holds(if i == 0 { "key.attack" } else { "key.use" }) {
        if i == 0 { game.left = false; } else { game.right = false; }
    }
}

fn checked_block(game: &Game<'_>, action: &BlockAction) -> bool {
    game.sunset_baritone_observation(action.pos).is_some_and(|(state, pending)| state == action.state_id && pending.is_none())
        && game.pick().is_some_and(|hit| hit.pos == action.pos) && !game.entity_targeted()
}

pub(super) fn apply(game: &mut Game<'_>, lease: owner::Lease, intent: &Intent, allow_break: bool, allow_place: bool) -> Result<(), String> {
    if !game.native_window() || !super::sunset::running() || !game.native_on("sunset") {
        return Err("Baritone input is unavailable while the client/runtime is off.".into());
    }
    validate(intent)?;
    if owner::current() != Some(lease) { return Err("Stale Baritone movement lease.".into()); }
    if intent.attack.is_some() && !allow_break || intent.use_block.is_some() && !allow_place {
        return Err("Baritone requested a disabled block action.".into());
    }
    let keys = movement(intent.movement)?;
    HELD.lock().unwrap_or_else(|e| e.into_inner()).lease = Some(lease);
    sprint_state(game, intent.movement & 64 != 0)?;
    if let Some(slot) = intent.selected_slot.filter(|&s| s != game.selected) {
        {
            let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
            let original = held.slot.map_or(game.selected, |(original, _)| original);
            held.slot = Some((original, slot));
        }
        super::sunset::hold(game, slot);
        if owner::current() != Some(lease) { return Err("Baritone was cancelled while selecting a tool.".into()); }
    }
    if intent.yaw.is_some() || intent.pitch.is_some() {
        let before = (game.yaw, game.pitch);
        game.yaw = intent.yaw.unwrap_or(game.yaw);
        game.pitch = intent.pitch.unwrap_or(game.pitch);
        HELD.lock().unwrap_or_else(|e| e.into_inner()).look = Some((before, (game.yaw, game.pitch)));
    }
    let attack = intent.attack.as_ref();
    if attack.is_some_and(|a| !checked_block(game, a)) || intent.use_block.as_ref().is_some_and(|a| !checked_block(game, a)) {
        return Err("Baritone block action no longer matches the live ray or prediction state.".into());
    }
    if let Some(a) = attack {
        let f = game.res.facts().get(horizon_world::registry::BlockStateId(a.state_id));
        let stack = game.inventory.get(game.selected).and_then(Option::as_ref);
        if f.hardness < 0.0 || !game.items().has_correct_tool_for_drops(stack, f.requires_tool, &f.name) {
            return Err("Baritone has no effective harvestable tool for this block.".into());
        }
    }
    owner::keys(game, lease, &keys);
    let before = game.sunset_baritone_click_counts();
    for (i, down) in [intent.attack.is_some(), intent.use_block.is_some()].into_iter().enumerate() {
        let changed = {
            let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
            let previous = std::mem::replace(&mut held.buttons[i], down);
            previous != down
        };
        if changed { button(game, i, down); }
    }
    let after = game.sunset_baritone_click_counts();
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    for i in 0..3 { held.clicks[i] = held.clicks[i].saturating_add(after[i].saturating_sub(before[i])); }
    Ok(())
}

pub(super) fn post_tick(game: &mut Game<'_>) {
    if !game.native_window() { return; }
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    held.clicks = [0; 3]; // The game's ordinary input tick consumed these clicks.
    if let Some((before, applied)) = held.look.take() {
        if (game.yaw, game.pitch) == applied { (game.yaw, game.pitch) = before; }
    }
}

pub(super) fn slot_changed(game: &Game<'_>) -> bool {
    HELD.lock().unwrap_or_else(|e| e.into_inner()).slot.is_some_and(|(_, last)| game.selected != last)
}

pub(super) fn release(game: &mut Game<'_>, lease: owner::Lease) {
    release_owned(game, lease, true);
}

pub(super) fn discard(game: &mut Game<'_>, lease: owner::Lease) {
    release_owned(game, lease, false);
}

fn release_owned(game: &mut Game<'_>, lease: owner::Lease, restore: bool) {
    if !game.native_window() { return; }
    let mut held = {
        let mut owned = HELD.lock().unwrap_or_else(|e| e.into_inner());
        if owned.lease != Some(lease) { return; }
        let held = *owned;
        *owned = Held { lease: None, buttons: [false; 2], clicks: [0; 3], look: None, slot: None };
        held
    };
    owner::release(game, lease);
    if !super::agent::sunset_goto::foreign_input(game).0.contains(&KeyCode::ControlLeft)
        && !super::sunset::native_held_input(game).0.contains(&KeyCode::ControlLeft)
    { let _ = sprint_state(game, false); }
    game.sunset_baritone_cancel_clicks(std::mem::take(&mut held.clicks));
    if let Some((before, applied)) = held.look.take().filter(|_| restore) {
        if (game.yaw, game.pitch) == applied { (game.yaw, game.pitch) = before; }
    }
    for i in 0..2 { if std::mem::take(&mut held.buttons[i]) { button(game, i, false); } }
    // Native release owns the abort and its sequence, including a release between simulation ticks.
    if !super::agent::sunset_goto::foreign_input(game).1[0] { game.request_remote_mining_stop(); }
    if let Some((original, last)) = held.slot.take().filter(|_| restore) {
        if game.selected == last && game.player.is_some() { super::sunset::hold(game, original); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::sprint::{Conditions, Keys, SprintState};
    #[test]
    fn baritone_sprint_off_stops_native_sprint_without_releasing_forward_or_retriggering() {
        let mut history = SprintState { previous_forward: true, trigger_time: 7, ..Default::default() };
        let mut sprinting = true;
        prepare_sprint(&mut history, false, sprinting, |value| { sprinting = value; Ok::<(), ()>(()) }).unwrap();
        assert!(!sprinting);
        assert_eq!(history.trigger_time, 0);
        for forward in [true, false, true] {
            history.tick(Keys { forward, ..Default::default() }, Conditions { sprinting, enough_food: true,
                on_ground: true, ..Default::default() }, 7,
                |value| { sprinting = value; Ok::<(), ()>(()) }).unwrap();
            assert!(!sprinting, "walking input must not inherit the engine's old sprint latch");
        }
        prepare_sprint(&mut history, true, sprinting, |_| Err::<(), ()>(())).unwrap();
        history.tick(Keys { forward: true, sprint: true, ..Default::default() }, Conditions { sprinting,
            enough_food: true, on_ground: true, ..Default::default() }, 7,
            |value| { sprinting = value; Ok::<(), ()>(()) }).unwrap();
        assert!(sprinting, "sprint-on still goes through the native mapped key and conditions");
    }
}
