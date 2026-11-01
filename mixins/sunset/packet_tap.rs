// The packets Crystal Aura and Anchor Aura learn from (blocks placed, players seen, hurt, equipped, gone; a new world
// or life), Jump Reset's knockback and the inventory modules' slot updates, copied as they arrive. The game handles them in the same frame; Sunset reads the copies at its next tick
// (crate::game::sunset::combat_packets). Kept to the last 4096, should no tick come to read them.

use horizon_protocol::packets::play::Clientbound as C;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

// Knockback Delay: the pushes on you (your velocity, an explosion's knockback) wait here, by chance, before the game
// takes them; the rest comes in on time. Sunset's tick says each tick how: whose pushes, the chance and the wait (ms).
struct Delay {
    me: i32,
    chance: f64,
    wait: [u64; 2],
    told: Instant,
    held: VecDeque<(Instant, super::Event)>,
    roll: u64,
}

static DELAY: Mutex<Option<Delay>> = Mutex::new(None);

pub fn delay(me: i32, chance: f64, wait: [u64; 2]) {
    let mut d = DELAY.lock().unwrap_or_else(|e| e.into_inner());
    let d = d.get_or_insert_with(|| Delay { me, chance, wait, told: Instant::now(), held: VecDeque::new(), roll: 0x9e37_79b9_7f4a_7c15 });
    (d.me, d.chance, d.wait, d.told) = (me, chance, wait, Instant::now());
}

pub fn delay_off() {
    if let Some(d) = DELAY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        d.told -= Duration::from_secs(1);
    }
}

pub fn hold(events: Vec<super::Event>) -> Vec<super::Event> {
    let mut guard = DELAY.lock().unwrap_or_else(|e| e.into_inner());
    let Some(d) = guard.as_mut() else {
        return events;
    };
    // Sunset's tick stopped coming (off, a menu, no world): nothing waits.
    let now = Instant::now();
    let on = now - d.told < Duration::from_millis(250);
    if d.held.is_empty() && !on {
        return events;
    }
    let mut out = Vec::with_capacity(events.len() + d.held.len());
    let due = d.held.iter().take_while(|(at, _)| !on || *at <= now).count();
    out.extend(d.held.drain(..due).map(|(_, e)| e));
    for event in events {
        if let super::Event::Packet(p) = &event {
            match &**p {
                // A new world or life: the old one's pushes are no good.
                C::Login(_) | C::Respawn(_) => d.held.clear(),
                // A teleport: what came before it first.
                C::PlayerPosition(_) => out.extend(d.held.drain(..).map(|(_, e)| e)),
                C::SetEntityMotion(m) if m.id == d.me => {}
                C::Explode(x) if x.knockback.is_some() => {}
                _ => {
                    out.push(event);
                    continue;
                }
            }
            if on && matches!(&**p, C::SetEntityMotion(_) | C::Explode(_)) {
                // One waiting already: this one goes after it; else the chance decides.
                let at = if let Some((last, _)) = d.held.back() {
                    Some(*last)
                } else {
                    d.roll ^= d.roll << 13;
                    d.roll ^= d.roll >> 7;
                    d.roll ^= d.roll << 17;
                    let [lo, hi] = d.wait;
                    let lucky = ((d.roll % 10_000) as f64) < d.chance * 100.0;
                    (lucky && hi > 0).then(|| now + Duration::from_millis(lo + (d.roll >> 16) % (hi - lo + 1)))
                };
                if let Some(at) = at {
                    d.held.push_back((at, event));
                    continue;
                }
            }
            out.extend(d.held.drain(..).map(|(_, e)| e));
        }
        out.push(event);
    }
    out
}
