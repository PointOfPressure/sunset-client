// Original actual-protocol adapter; no synthetic packets or game mutations.
use horizon_protocol::packets::play::Clientbound as C;
pub fn correction(packet: &C, from: [f64; 3]) -> Option<[f64; 3]> {
    let C::PlayerPosition(p) = packet else {
        return None;
    };
    if !from.iter().chain(p.position.iter()).all(|v| v.is_finite()) {
        return None;
    }
    let to = std::array::from_fn(|i| {
        p.position[i]
            + if p.relatives & (1 << i) != 0 {
                from[i]
            } else {
                0.
            }
    });
    to.iter().all(|v| v.is_finite()).then_some(to)
}
pub fn own_damage_cause(packet: &C, own: i32) -> Option<u32> {
    let C::DamageEvent(p) = packet else {
        return None;
    };
    if p.id != own {
        return None;
    }
    u32::try_from(p.cause?).ok()
}
pub fn totem(packet: &C) -> Option<i32> {
    let C::EntityEvent(p) = packet else {
        return None;
    };
    (p.event == 35).then_some(p.id)
}
pub fn time(packet: &C) -> Option<i64> {
    let C::SetTime(p) = packet else {
        return None;
    };
    Some(p.game_time)
}
pub fn blast(packet: &C) -> Option<[f64; 3]> {
    let C::Explode(p) = packet else {
        return None;
    };
    p.center.iter().all(|v| v.is_finite()).then_some(p.center)
}

// Native preserved component-patch format. Registry id38 is PiercingWeapon
// in the exact pinned protocol's item/skip.rs. No component bytes are guessed.
pub fn ordinary_component_hand(mut piercing: bool, patch: Option<&[u8]>) -> Option<bool> {
    use horizon_protocol::buf::Reader;
    if let Some(bytes) = patch {
        if bytes.len() > 8192 {
            return None;
        }
        let mut input = Reader::new(bytes);
        let added = input.count(64).ok()?;
        let removed = input.count(64).ok()?;
        if added + removed > 64 {
            return None;
        }
        let mut seen = Vec::with_capacity(added + removed);
        for _ in 0..added {
            let id = input.varint().ok()?;
            if id < 0 || seen.contains(&id) {
                return None;
            }
            seen.push(id);
            let size = input.count(input.remaining()).ok()?;
            input.bytes(size).ok()?;
            if id == 38 {
                piercing = true;
            }
        }
        for _ in 0..removed {
            let id = input.varint().ok()?;
            if id < 0 || seen.contains(&id) {
                return None;
            }
            seen.push(id);
            if id == 38 {
                piercing = false;
            }
        }
        if input.remaining() != 0 {
            return None;
        }
    }
    Some(!piercing)
}
