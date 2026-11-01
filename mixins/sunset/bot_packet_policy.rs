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
