// The packets Crystal Aura and Anchor Aura learn from (blocks placed, players seen, hurt, equipped, gone; a new world
// or life), Jump Reset's knockback and the inventory modules' slot updates, copied as they arrive. The game handles them in the same frame; Sunset reads the copies at its next tick
// (crate::game::sunset::combat_packets). Kept to the last 4096, should no tick come to read them.

use horizon_protocol::packets::play::Clientbound as C;
use std::sync::Mutex;

static SEEN: Mutex<Vec<C>> = Mutex::new(Vec::new());

pub fn seen(events: &[super::Event]) {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    for event in events {
        if let super::Event::Packet(p) = event
            && matches!(
                **p,
                C::Login(_)
                    | C::Respawn(_)
                    | C::Disconnect(_)
                    | C::PlayerPosition(_)
                    | C::BlockUpdate(_)
                    | C::SectionBlocksUpdate(_)
                    | C::AddEntity(_)
                    | C::RemoveEntities(_)
                    | C::SetEntityData(_)
                    | C::SetEquipment(_)
                    | C::DamageEvent(_)
                    | C::EntityEvent(_)
                    | C::SetEntityMotion(_)
                    | C::ContainerSetSlot(_)
                    | C::ContainerSetContent(_)
                    | C::SetPlayerInventory(_)
                    | C::SetCursorItem(_)
                    | C::OpenScreen(_)
            )
            && seen.len() < 4096
        {
            seen.push((**p).clone());
        }
    }
}

pub fn take() -> Vec<C> {
    std::mem::take(&mut *SEEN.lock().unwrap_or_else(|e| e.into_inner()))
}
