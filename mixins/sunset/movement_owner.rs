// One generation-tagged lease for Sunset's ordinary movement drivers. Foreign agent/script input is never taken.
use super::Game;
use horizon_client::held::Source;
use std::sync::Mutex;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Owner { AutoWalk, Goto, Baritone }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Lease { pub owner: Owner, pub generation: u64 }

struct Broker { generation: u64, lease: Option<Lease>, keys: Vec<KeyCode> }
impl Broker {
    fn set_keys(&mut self, keys: &mut horizon_client::held::HeldKeys, lease: Lease, wanted: &[KeyCode]) -> bool {
        if self.lease != Some(lease) { return false; }
        for k in &self.keys { if !wanted.contains(k) { keys.release(k, Source::Agent); } }
        for &k in wanted { keys.press(k, Source::Agent); }
        self.keys = wanted.to_vec();
        true
    }
    fn clear(&mut self, keys: &mut horizon_client::held::HeldKeys, lease: Lease) {
        if self.lease != Some(lease) { return; }
        self.lease = None;
        self.generation = self.generation.wrapping_add(1).max(1);
        for k in self.keys.drain(..) { keys.release(&k, Source::Agent); }
    }
    fn retire(&mut self, keys: &mut horizon_client::held::HeldKeys, lease: Lease, foreign: &[KeyCode]) {
        if self.lease != Some(lease) { return; }
        self.clear(keys, lease);
        for &key in foreign { keys.press(key, Source::Agent); }
    }
}
static BROKER: Mutex<Broker> = Mutex::new(Broker { generation: 0, lease: None, keys: Vec::new() });
pub(super) const MOVE_KEYS: [KeyCode; 7] = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD,
    KeyCode::Space, KeyCode::ShiftLeft, KeyCode::ControlLeft];

pub(super) fn current() -> Option<Lease> { BROKER.lock().unwrap_or_else(|e| e.into_inner()).lease }

pub(super) fn conflict(mods: &[super::sunset::Module]) -> Option<&'static str> {
    mods.iter().find(|m| (m.name == "Inventory Manager" && m.b("Auto Armor")) || m.on && matches!(m.name,
        "Flight" | "Speed" | "Scaffold" | "Packet Mine" | "Nuker" | "Kill Aura" | "Trigger Bot" |
        "Crystal Aura" | "Anchor Aura" | "Cart" | "PvP Bot" | "Auto Mace" | "Elytra Target" | "Auto Spear" |
        "Auto Tool" | "Auto Eat" | "Auto Gap" | "Inventory Manager" | "Auto Replenish" | "Chest Swap" |
        "Offhand" | "Anti AFK" | "Parkour" | "Auto Jump" | "Sneak" | "Elytra Fly" | "Elytra Boost" |
        "Glide" | "Spider" | "High Jump" | "Long Jump" | "Jesus" | "Blink" | "Freecam" |
        "Aim Assist" | "Bow Aim" | "Auto City" | "Hole Filler" | "Liquid Filler" | "Burrow" |
        "Tap Use" | "Wind Jump" | "Pearl Catch" | "Middle Click Extra" | "Auto Clicker" |
        "Fast Place" | "Step" | "Anti Void" | "No Fall" | "Fast Break" | "Auto Fish" | "Fast Use" |
        "BlockIn" | "Self Trap" | "Auto Trap" | "Self Web" | "Auto Web" | "Auto EXP" | "Crystal Optimizer" |
        "Bow Aimbot" | "Auto Shearer" | "Auto Nametag" | "Auto Breed" | "Auto Mount" | "Flamethrower" | "Spawn Proofer" |
        "Shield" | "RF Click" | "Rotations" | "Anvil" | "Quiver" | "Ghost Hand" | "Air Place"
    )).map(|m| m.name)
}

pub(super) fn can_acquire(game: &Game<'_>) -> Result<(), String> {
    if !game.native_window() || !super::sunset::running() || !game.native_on("sunset") {
        return Err("Sunset is off.".into());
    }
    if super::agent::sunset_goto::foreign_busy(game) {
        return Err("Another Horizon agent owns movement; stop it first.".into());
    }
    let native = super::sunset::native_held_input(game);
    if MOVE_KEYS.iter().any(|k| native.0.contains(k)) || !native.1.is_empty() {
        return Err("Let go of movement and attack/use controls first.".into());
    }
    if foreign_keys(game) {
        return Err("A script holds movement; release its controls first.".into());
    }
    Ok(())
}

pub(super) fn foreign_keys(game: &Game<'_>) -> bool {
    let broker = BROKER.lock().unwrap_or_else(|e| e.into_inner());
    MOVE_KEYS.iter().any(|k| game.keys.held_by(k, Source::Agent)
        && !broker.keys.contains(k) && !super::agent::sunset_goto::owns(game))
}

pub(super) fn acquire(game: &mut Game<'_>, owner: Owner) -> Result<Lease, String> {
    can_acquire(game)?;
    if owner != Owner::Baritone { super::sunset_baritone::stop(game, super::sunset_baritone::StopReason::Conflict); }
    if owner != Owner::Goto { super::agent::sunset_goto::cancel(game, "stopped: another Sunset movement job started"); }
    if owner != Owner::AutoWalk {
        let changed = super::sunset::with(|s| {
            if let Some(m) = s.modules.iter_mut().find(|m| m.name == "Auto Walk") {
                let old = m.on; m.on = false; old
            } else { false }
        });
        if changed { super::sunset::save(game); }
    }
    let previous = current();
    if let Some(lease) = previous { release(game, lease); }
    let mut broker = BROKER.lock().unwrap_or_else(|e| e.into_inner());
    broker.generation = broker.generation.wrapping_add(1).max(1);
    let lease = Lease { owner, generation: broker.generation };
    broker.lease = Some(lease);
    Ok(lease)
}

pub(super) fn keys(game: &mut Game<'_>, lease: Lease, wanted: &[KeyCode]) -> bool {
    if !game.native_window() || !super::sunset::running() || !game.native_on("sunset") { return false; }
    let mut broker = BROKER.lock().unwrap_or_else(|e| e.into_inner());
    broker.set_keys(&mut game.keys, lease, wanted)
}

pub(super) fn release(game: &mut Game<'_>, lease: Lease) {
    if !game.native_window() { return; }
    let (foreign, _) = super::agent::sunset_goto::foreign_input(game);
    let mut broker = BROKER.lock().unwrap_or_else(|e| e.into_inner());
    broker.retire(&mut game.keys, lease, &foreign);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baritone_cancellation_releases_owned_sources_and_preserves_keyboard_pad() {
        let lease = Lease { owner: Owner::Baritone, generation: 1 };
        let mut broker = Broker { generation: 1, lease: Some(lease), keys: Vec::new() };
        let mut keys = horizon_client::held::HeldKeys::default();
        keys.press(KeyCode::KeyW, Source::Keyboard);
        keys.press(KeyCode::Space, Source::Pad);
        assert!(broker.set_keys(&mut keys, lease, &[KeyCode::KeyW, KeyCode::Space, KeyCode::ShiftLeft]));
        broker.clear(&mut keys, lease);
        assert!(keys.held_by(&KeyCode::KeyW, Source::Keyboard));
        assert!(keys.held_by(&KeyCode::Space, Source::Pad));
        assert!(!keys.held_by(&KeyCode::KeyW, Source::Agent));
        assert!(!keys.contains(&KeyCode::ShiftLeft));
        assert!(broker.lease.is_none() && broker.generation != lease.generation);
        let after = keys.clone();
        broker.clear(&mut keys, lease);
        assert_eq!(keys, after, "repeated stop must be harmless");
    }
    #[test]
    fn baritone_stale_lease_cannot_press_or_release_new_owner() {
        let old = Lease { owner: Owner::Baritone, generation: 1 };
        let new = Lease { owner: Owner::AutoWalk, generation: 2 };
        let mut broker = Broker { generation: 2, lease: Some(new), keys: Vec::new() };
        let mut keys = horizon_client::held::HeldKeys::default();
        assert!(broker.set_keys(&mut keys, new, &[KeyCode::KeyW]));
        assert!(!broker.set_keys(&mut keys, old, &[KeyCode::KeyS]));
        broker.clear(&mut keys, old);
        assert_eq!(broker.lease, Some(new));
        assert!(keys.held_by(&KeyCode::KeyW, Source::Agent));
        assert!(!keys.contains(&KeyCode::KeyS));
    }
    #[test]
    fn baritone_retirement_preserves_a_foreign_tasks_shared_agent_source() {
        let lease = Lease { owner: Owner::Baritone, generation: 1 };
        let mut broker = Broker { generation: 1, lease: Some(lease), keys: Vec::new() };
        let mut keys = horizon_client::held::HeldKeys::default();
        broker.set_keys(&mut keys, lease, &[KeyCode::KeyW, KeyCode::Space]);
        // ctl move pressed W and S before installing its task; both use Horizon's single Agent source bit.
        keys.press(KeyCode::KeyW, Source::Agent);
        keys.press(KeyCode::KeyS, Source::Agent);
        broker.retire(&mut keys, lease, &[KeyCode::KeyW, KeyCode::KeyS]);
        assert!(keys.held_by(&KeyCode::KeyW, Source::Agent));
        assert!(keys.held_by(&KeyCode::KeyS, Source::Agent));
        assert!(!keys.contains(&KeyCode::Space));
        assert!(broker.lease.is_none());
    }
}

pub(super) fn auto_walk(game: &mut Game<'_>, on: bool) {
    if on && game.native_playing() && !game.screen_over_playing() && game.sleeping.is_none() && !super::sunset::gui_open() {
        let lease = current().filter(|l| l.owner == Owner::AutoWalk)
            .or_else(|| acquire(game, Owner::AutoWalk).ok());
        if let Some(lease) = lease { keys(game, lease, &[KeyCode::KeyW]); }
    } else if let Some(lease) = current().filter(|l| l.owner == Owner::AutoWalk) { release(game, lease); }
}
