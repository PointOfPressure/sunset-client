// An item's damage as the server's component patch gives it.
use super::sunset_faceplace_policy::ArmorCondition;
use horizon_protocol::{buf::Reader, item::component};
#[derive(Clone, Copy, Debug)]
pub struct Prototype {
    pub maximum: Option<u32>,
    pub damage_present: bool,
    pub unbreakable: bool,
}
pub fn resolve(base: Prototype, decoded_damage: u32, patch: Option<&[u8]>) -> Option<ArmorCondition> {
    let mut maximum = base.maximum;
    let mut damage = decoded_damage;
    let mut damage_present = base.damage_present;
    let mut unbreakable = base.unbreakable;
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
        let mut seen = [-1; 64];
        let mut used = 0;
        for _ in 0..added {
            let id = input.varint().ok()?;
            if id < 0 || seen[..used].contains(&id) {
                return None;
            }
            seen[used] = id;
            used += 1;
            let count = input.count(input.remaining()).ok()?;
            let payload = input.bytes(count).ok()?;
            let mut value = Reader::new(payload);
            match id {
                component::MAX_DAMAGE => {
                    maximum = Some(u32::try_from(value.varint().ok()?).ok()?);
                }
                component::DAMAGE => {
                    damage = u32::try_from(value.varint().ok()?).ok()?;
                    damage_present = true;
                }
                component::UNBREAKABLE => {
                    unbreakable = true;
                }
                _ => continue,
            }
            if value.remaining() != 0 {
                return None;
            }
        }
        for _ in 0..removed {
            let id = input.varint().ok()?;
            if id < 0 || seen[..used].contains(&id) {
                return None;
            }
            seen[used] = id;
            used += 1;
            match id {
                component::MAX_DAMAGE => maximum = None,
                component::DAMAGE => damage_present = false,
                component::UNBREAKABLE => unbreakable = false,
                _ => {}
            }
        }
        if input.remaining() != 0 {
            return None;
        }
    }
    Some(ArmorCondition {
        damageable: damage_present && !unbreakable && maximum.is_some_and(|v| v > 0),
        maximum: maximum.unwrap_or(0),
        lost: damage,
    })
}
