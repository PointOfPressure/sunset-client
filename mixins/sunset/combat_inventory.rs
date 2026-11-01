// Original inventory utilities built on Horizon's predicted menu engine and
// authoritative incoming slot packets. No carried-stack pickup sequences.
use super::{Game, Screen};
use super::sunset::{self, Category, Module, hold, with};
use super::sunset_combat_control as control;
use super::sunset_inventory_policy::{self as policy, Failure, Status, Swap, OFFHAND};
use super::sunset_util as util;
use horizon_client::items::ItemStack;
use horizon_protocol::packets::{play::{self, Clientbound as C, Serverbound as S}};
use std::{any::Any, sync::{Arc, Mutex}, time::{Duration, Instant}};

const AUTO: &str = "Auto Totem";
const INV: &str = "Auto Inventory Totem";
const HOVER: &str = "Hover Totem";
const OFF: &str = "Offhand";
const XP: &str = "FastXP";
const THROW: &str = "Key Throw";
const REPAIR: &str = "Auto EXP";
const OWNERS: [&str; 7] = [AUTO, INV, HOVER, OFF, XP, THROW, REPAIR];

// Replaces the existing factory entry; never append a second Auto Totem name.
pub(super) fn auto_totem_defaults() -> Module {
    Module::new(AUTO, Category::Combat, "Refills only an empty offhand with a server-confirmed totem swap. Refuses a carried stack or another open container.")
        .num("Refill delay", 3.0, 1.0, 10.0, 1.0)
        .bool("Use hotbar totems", false)
}

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(INV, Category::Combat, "Refills totems with confirmed inventory or hotbar swaps. Double Hand also refills the selected hand while health is low. Auto Totem and this module are exclusive.")
            .pick("Mode", &["Inventory", "Hotbar Swap"])
            .bool("Hotbar refill", false).num("Hotbar slot", 1.0, 1.0, 9.0, 1.0)
            .num("Reaction delay", 0.0, 0.0, 10.0, 1.0)
            .num("Swap delay", 1.0, 0.0, 10.0, 1.0)
            .num("Close delay", 0.0, 0.0, 10.0, 1.0)
            .num("Switch back delay", 2.0, 0.0, 10.0, 1.0)
            .bool("Double Hand", false),
        Module::new(HOVER, Category::Combat, "Moves only the totem under your inventory pointer into the offhand or configured hotbar slot. Waits for both server slot updates.")
            .bool("Hotbar refill", false).num("Hotbar slot", 1.0, 1.0, 9.0, 1.0),
        Module::new(XP, Category::Combat, "Shortens the native item-use delay only while you hold use with an experience bottle. Yields to other combat actions.")
            .num("Delay", 0.0, 0.0, 3.0, 1.0),
        Module::new(THROW, Category::Combat, "Throws one pearl or wind charge per key press and returns the selected slot. Never repeats a held key.")
            .text("Pearl key", "None").text("Wind charge key", "None"),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase { Reaction, SwapDelay, Await, Close, Return }

struct Job {
    owner: &'static str,
    settings: Module,
    slots: [usize; 2],
    before: [Option<ItemStack>; 2],
    menu: (i32, String),
    wire: [Option<usize>; 2],
    phase: Phase,
    due: u64,
    began: Instant,
    swap: Option<Swap<ItemStack>>,
    hotbar: bool,
    opened: bool,
    restore: Option<(usize, usize)>,
    pass_swaps: u8,
    unsafe_ticks: u8,
    revision:Option<i32>,
}

#[derive(Clone, Copy)]
enum Throw { Pearl, Wind }
#[derive(Clone, Copy)]
struct Press { kind: Throw, slot: usize, tick: u64 }

#[derive(Default)]
struct State {
    connection: Option<(i32, super::Dim, Arc<dyn Any + Send + Sync>)>,
    tick: u64,
    job: Option<Job>,
    failures: Vec<(&'static str, Failure<ItemStack>)>,
    auto_on: bool,
    inv_on: bool,
    pearl: policy::Edge,
    wind: policy::Edge,
    press: Option<Press>,
    next: u64,
    queued_use: Option<(&'static str, Instant)>,
    repairing: bool,
    hand: policy::Hand,
    pace: policy::RefillPace,
    epoch: Option<Instant>,
    random: u64,
    previous_source: Option<usize>,
    last_action:Option<u64>,
    auto_retry:policy::RefillRetry,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
static SWAP_REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SWAP_CONFIRMED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SWAP_FAILED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SWAP_LAST_ACK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

pub(super) fn debug() -> serde_json::Value {
    state(|s| serde_json::json!({
        "owner":s.job.as_ref().map(|j|j.owner),
        "phase":s.job.as_ref().map(|j|match j.phase { Phase::Reaction=>"reaction",Phase::SwapDelay=>"swap-delay",Phase::Await=>"server-ack",Phase::Close=>"inventory-close",Phase::Return=>"hotbar-return" }),
        "slots":s.job.as_ref().map(|j|j.slots),
        "ack_bits":s.job.as_ref().and_then(|j|j.swap.as_ref()).map(|p|p.confirmations()),
        "failed_owners":s.failures.len(),
        "double_hand":s.hand.held,
        "queued_throw":s.press.is_some(),
        "queued_use":s.queued_use.map(|p|p.0),
        "refill_attempts":s.auto_retry.attempts,
        "swap_requests":SWAP_REQUESTS.load(std::sync::atomic::Ordering::Relaxed),
        "swap_confirmed":SWAP_CONFIRMED.load(std::sync::atomic::Ordering::Relaxed),
        "swap_failed":SWAP_FAILED.load(std::sync::atomic::Ordering::Relaxed),
        "swap_last_ack_bits":SWAP_LAST_ACK.load(std::sync::atomic::Ordering::Relaxed),
    }))
}

pub(super) fn toggled(game: &mut Game<'_>, name: &str, on: bool) {
    if on && name == AUTO { util::toggle_off(game, INV); }
    if on && name == INV { util::toggle_off(game, AUTO); }
}

pub(super) fn reset() {
    if let Some(owner)=state(|s|s.job.as_ref().map(|j|j.owner)){control::release(owner);}
    for owner in OWNERS { sunset::cancel_turns(owner); control::release(owner); }
    state(|s| *s = State::default());
}

pub(super) fn cancel(game: &mut Game<'_>) {
    let same = state(|s| s.connection.as_ref().is_some_and(|old| same_session(game, old)));
    if same {
        if let Some(job) = state(|s| s.job.take()) { restore(game, &job); }
        if let Some(slot) = state(|s| s.hand.restore(game.selected)) { hold(game, slot); }
    }
    reset();
}

fn session(game: &Game<'_>) -> Option<(i32, super::Dim, Arc<dyn Any + Send + Sync>)> {
    let r = game.remote.as_ref()?;
    Some((r.entity_id, game.dim, r.session.counters.clone()))
}
fn same_session(game: &Game<'_>, old: &(i32, super::Dim, Arc<dyn Any + Send + Sync>)) -> bool {
    game.remote.as_ref().is_some_and(|r| r.entity_id == old.0 && game.dim == old.1
        && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ())
}
fn alive(game: &Game<'_>) -> bool {
    game.native_window() && game.native_on(sunset::ID) && game.remote.is_some()
        && !game.spectator() && !game.creative && !sunset::gui_open()
        && matches!(game.screen, Screen::Playing | Screen::Inventory)
        && game.selected < 9 && game.player.as_ref().is_some_and(|p|
            p.health.health().is_finite() && p.health.health() > 0.0)
}
fn plain(game: &Game<'_>) -> bool {
    alive(game) && game.screen == Screen::Playing && game.remote_menu().0 == 0
        && game.carried.is_none() && game.using.is_none()
        && game.mining.is_none() && super::remote::sunset_remote::breaking(game).is_none()
}
fn stack(game: &mut Game<'_>, slot: usize) -> Option<ItemStack> { game.slot_mut(slot).clone() }
fn is_totem(stack: &Option<ItemStack>) -> bool {
    stack.as_ref().is_some_and(|s| &*s.item == "minecraft:totem_of_undying" && s.count == 1)
}
fn find_totem(game: &Game<'_>, from: usize) -> Option<usize> {
    (from..36).find(|&i| is_totem(&game.inventory[i]))
}
fn random(s:&mut State)->u64 {
    s.random=s.random.wrapping_add(0x9e3779b97f4a7c15);
    let mut n=s.random;n=(n^(n>>30)).wrapping_mul(0xbf58476d1ce4e5b9);
    n=(n^(n>>27)).wrapping_mul(0x94d049bb133111eb);n^(n>>31)
}
fn micros(s:&mut State)->u64 {
    s.epoch.get_or_insert_with(Instant::now).elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}
fn slot_setting(m: &Module) -> usize { policy::delay(m.n("Hotbar slot"), 9).clamp(1, 9) as usize - 1 }

fn restore(game: &mut Game<'_>, job: &Job) {
    if let Some((before, ours)) = job.restore {
        if game.selected == ours && before < 9 { hold(game, before); }
    }
    // Close only the player inventory this job itself opened. Never close a
    // container or return a user-carried stack behind the user's back.
    if job.opened && game.screen == Screen::Inventory && game.remote_menu().0 == 0 && game.carried.is_none() {
        game.close_container();
        game.screen = Screen::Playing;
        game.capture(true);
    }
}

fn fail(game: &mut Game<'_>, job: Job) {
    SWAP_FAILED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    restore(game, &job);
    state(|s| {
        s.failures.retain(|(owner, _)| *owner != job.owner);
        if job.owner==AUTO {
            let now=micros(s);s.auto_retry.failed(now,super::remote::sunset_combat_remote::latency(game));
        }else{s.failures.push((job.owner, Failure { slots: job.slots, before: job.before }));}
        s.next = game.ticks.saturating_add(4);
    });
    control::release(job.owner);
}

fn schedule(game: &mut Game<'_>, m: &Module, owner: &'static str, from: usize, to: usize, hotbar: bool) -> bool {
    if from == to || !(to < 9 || to == OFFHAND) || game.carried.is_some() { return false; }
    let before = [stack(game, from), stack(game, to)];
    if !before.iter().flatten().all(|s| game.res.items().get(&s.item)
        .is_some_and(|info| s.count > 0 && s.count <= info.max_stack)) { return false; }
    if before[0].is_none() || before[0] == before[1]
        || state(|s| s.failures.iter().any(|(o, f)| *o == owner && f.blocks([from, to], &before))) { return false; }
    let menu = game.remote_menu();
    let wire = [Game::menu_slot_index(&menu.1, from), Game::menu_slot_index(&menu.1, to)];
    if wire[0].is_none() || (!hotbar && to != OFFHAND && wire[1].is_none()) || !control::claim(owner) { return false; }
    let reaction = if owner == INV { policy::delay(m.n("Reaction delay"), 10) }
        else if owner == AUTO || owner == HOVER { 0 }
        else { policy::delay(m.n("Item switch delay"), 20) };
    state(|s| s.job = Some(Job { owner, settings: m.clone(), slots: [from, to], before, menu, wire,
        phase: Phase::Reaction, due: game.ticks.saturating_add(reaction), began: Instant::now(),
        swap: None, hotbar, opened: false, restore: None, pass_swaps:0, unsafe_ticks:0,revision:None }));
    run_job(game,std::slice::from_ref(m));
    true
}

fn run_job_step(game: &mut Game<'_>, mods: &[Module]) -> bool {
    let Some(mut job) = state(|s| s.job.take()) else { return false; };
    if !alive(game) || !mods.iter().any(|m| m.on && m == &job.settings)
        || game.remote_menu() != job.menu || game.carried.is_some()
        || job.restore.is_some_and(|(_,ours)|game.selected!=ours)
        || (job.owner == HOVER && matches!(job.phase, Phase::Reaction | Phase::SwapDelay)
            && game.inventory_target().0 != Some(job.slots[0]))
        // Five-second maximum server confirmation plus a one-second local
        // reaction/close margin. Reset at each actual bounded pass operation.
        || job.began.elapsed() > Duration::from_secs(6) || !control::claim(job.owner) {
        fail(game, job);
        return true;
    }
    if game.ticks < job.due { state(|s| s.job = Some(job)); return true; }
    match job.phase {
        Phase::Reaction => {
            if job.owner == INV && !job.hotbar && game.screen == Screen::Playing {
                game.reset_inventory_clicks();
                game.screen = Screen::Inventory;
                game.capture(false);
                job.opened = true;
            }
            if job.hotbar {
                let from=job.slots[0];
                if from>=9 {fail(game,job);return true;}
                job.restore=Some((game.selected,from));hold(game,from);
            }
            job.phase = Phase::SwapDelay;
            let wait=if job.owner == INV { policy::delay(job.settings.n("Swap delay"), 10).max(u64::from(job.opened)) } else { 0 };
            job.due = game.ticks.saturating_add(wait);
            state(|s| s.job = Some(job));
        }
        Phase::SwapDelay => {
            if (job.owner==INV || job.owner==HOVER) && !job.hotbar
                && !super::remote::sunset_combat_remote::quiet(game) {
                job.unsafe_ticks=job.unsafe_ticks.saturating_add(1);
                if job.unsafe_ticks>10 {fail(game,job);}else{
                    job.due=game.ticks.saturating_add(1);state(|s|s.job=Some(job));
                }
                return true;
            }
            if [stack(game, job.slots[0]), stack(game, job.slots[1])] != job.before {
                fail(game, job); return true;
            }
            job.swap = Swap::new(job.slots, job.before.clone(), game.ticks);
            if job.swap.is_none() { fail(game, job); return true; }
            SWAP_LAST_ACK.store(0, std::sync::atomic::Ordering::Relaxed);
            job.phase = Phase::Await;
            job.began=Instant::now();
            job.revision=super::remote::sunset_combat_remote::container_state(game);
            if job.hotbar {
                let from = job.slots[0];
                if from >= 9 || job.slots[1] != OFFHAND { fail(game, job); return true; }
                state(|s| s.job = Some(job));
                game.send(S::PlayerAction(play::PlayerAction { action: 7, pos: [0; 3], direction: 0, sequence: 0 }));
                SWAP_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else {
                let from = job.slots[0];
                let to = job.slots[1];
                let menu = job.menu.clone();
                state(|s| s.job = Some(job));
                if super::remote::sunset_combat_remote::swap_unpredicted(game, &menu, from, to) {
                    SWAP_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if state(|s|s.job.as_ref().is_some_and(|j|j.owner==AUTO)){state(|s|s.auto_retry.sent());}
                } else {
                    if let Some(job) = state(|s| s.job.take()) { fail(game, job); }
                }
            }
        }
        Phase::Await => {
            let timeout=policy::confirmation_ticks(super::remote::sunset_combat_remote::latency(game));
            match job.swap.as_ref().map_or(Status::Rejected, |s| s.status(game.ticks, timeout)) {
                Status::Waiting => state(|s| s.job = Some(job)),
                Status::Confirmed => {
                    SWAP_CONFIRMED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    job.pass_swaps=job.pass_swaps.saturating_add(1);
                    // Each operation waits for both genuine server slot values;
                    // the pass may then refill the configured hotbar once.
                    let to=slot_setting(&job.settings);
                    let follow=if job.owner==INV && !job.hotbar && job.pass_swaps<2
                        && job.settings.b("Hotbar refill") && !is_totem(&game.inventory[to]) {
                        find_totem(game,9).map(|from|(from,to))
                    }else{None};
                    if let Some((from,to))=follow {
                        job.slots=[from,to];job.before=[stack(game,from),stack(game,to)];
                        job.wire=[Game::menu_slot_index(&job.menu.1,from),Game::menu_slot_index(&job.menu.1,to)];
                        job.swap=None;job.phase=Phase::SwapDelay;
                        job.due=game.ticks.saturating_add(policy::delay(job.settings.n("Swap delay"),10).max(1));
                    }else{
                        job.phase=if job.hotbar{Phase::Return}else{Phase::Close};
                        let wait=if job.owner==INV{policy::finish_delay(job.hotbar,job.settings.n("Close delay"),job.settings.n("Switch back delay"))}else{0};
                        job.due=game.ticks.saturating_add(wait);
                    }
                    state(|s| s.job = Some(job));
                }
                _ => fail(game, job),
            }
        }
        Phase::Close | Phase::Return => {
            restore(game, &job);
            state(|s| s.next = game.ticks.saturating_add(if job.owner==INV && !job.hotbar{4}else{1}));
            control::release(job.owner);
        }
    }
    true
}

// Resolve only already-due local stage transitions; ACK waiting never spins.
fn run_job(game:&mut Game<'_>,mods:&[Module])->bool {
    let mut used=false;
    for _ in 0..4 {
        let before=state(|s|s.job.as_ref().map(|j|j.phase));
        if !run_job_step(game,mods){break;}
        used=true;
        let next=state(|s|s.job.as_ref().map(|j|(j.phase,j.due)));
        if next.is_none_or(|(phase,due)|phase==Phase::Await || Some(phase)==before || due>game.ticks){break;}
    }
    used
}

/// Shared refill submission for a bot's current, enabled module snapshot.
/// No stack prediction: completion still needs both authoritative payload ACKs.
pub(super) fn submit_refill(game:&mut Game<'_>,module:&Module,owner:&'static str,from:usize,to_hotbar:usize)->bool {
    if !module.on || !(9..36).contains(&from) || to_hotbar>=9 || !plain(game)
        || !super::remote::sunset_combat_remote::quiet(game)
        || state(|s|s.job.is_some() || s.queued_use.is_some()) {return false;}
    if state(|s|s.connection.as_ref().is_none_or(|old|!same_session(game,old))) {
        reset();state(|s|s.connection=session(game));
    }
    schedule(game,module,owner,from,to_hotbar,false)
}
pub(super) fn busy_for(_owner:&str)->bool {state(|s|s.job.is_some() || s.queued_use.is_some())}

// Called at HEAD of the native packet handler. Values come from server packet
// payloads, not the optimistically predicted inventory already on the client.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    state(|s| {
        if !s.connection.as_ref().is_some_and(|old| same_session(game, old)) { return; }
        let Some(job) = s.job.as_mut().filter(|j| j.phase == Phase::Await) else { return; };
        let revision=match packet {
            C::ContainerSetSlot(p) if p.container==job.menu.0=>Some(p.state),
            C::ContainerSetContent(p) if p.container==job.menu.0=>Some(p.state),
            _=>None,
        };
        if let Some(revision)=revision {
            if job.revision.is_some_and(|previous|!policy::revision_current(previous,revision)){return;}
            job.revision=Some(revision);
        }
        let Some(swap) = job.swap.as_mut() else { return; };
        let before = swap.confirmations();
        let mut at_wire = |container: i32, wire: usize, item: &Option<horizon_protocol::item::Stack>| {
            for i in 0..2 {
                let matches = (container == job.menu.0 && job.wire[i] == Some(wire))
                    || (container == 0 && policy::player_wire(job.slots[i]) == Some(wire));
                if matches { swap.observe(job.slots[i], &game.to_stack(item)); }
            }
        };
        match packet {
            C::ContainerSetSlot(p) if p.slot >= 0 => at_wire(p.container, p.slot as usize, &p.item),
            C::ContainerSetContent(p) => {
                for i in 0..2 {
                    let wire = if p.container == job.menu.0 { job.wire[i] }
                        else if p.container == 0 { policy::player_wire(job.slots[i]) } else { None };
                    if let Some(item) = wire.and_then(|wire| p.items.get(wire)) {
                        swap.observe(job.slots[i], &game.to_stack(item));
                    }
                }
                if p.carried.is_some() { swap.reject(); }
            }
            C::SetPlayerInventory(p) => {
                if let Some(slot) = policy::player_internal(p.slot) { swap.observe(slot, &game.to_stack(&p.item)); }
            }
            C::SetCursorItem(p) if p.item.is_some() => swap.reject(),
            C::OpenScreen(_) => swap.reject(),
            _ => {},
        }
        if swap.confirmations() != before {
            SWAP_LAST_ACK.store(u64::from(swap.confirmations()), std::sync::atomic::Ordering::Relaxed);
        }
    });
}

fn auto_totem(game: &mut Game<'_>, m: &Module) -> bool {
    if game.offhand.is_some(){state(|s|{s.pace.clear();s.auto_retry.reset();});return false;}
    if !plain(game) || !state(|s|{let now=micros(s);s.auto_retry.ready(now)}){state(|s|s.pace.clear());return false;}
    let mut hotbar:Vec<_>=(0..9).filter(|i|is_totem(&game.inventory[*i])).collect();
    let mut stored:Vec<_>=(9..36).filter(|i|is_totem(&game.inventory[*i])).collect();
    let candidate=state(|s|{
        let sample=random(s);
        if m.b("Use hotbar totems") && !hotbar.is_empty(){Some(hotbar.swap_remove(sample as usize%hotbar.len()))}
        else{policy::nearby_source(&mut stored,s.previous_source,sample)}
    });
    let source=state(|s|{
        let now=micros(s);
        let samples=[random(s),random(s),random(s)];
        s.pace.step(now,policy::delay(m.n("Refill delay"),10),candidate,
            |i|i<36 && (i>=9 || m.b("Use hotbar totems")) && is_totem(&game.inventory[i]),samples)
    });
    source.is_some_and(|from|{
        let accepted=schedule(game,m,AUTO,from,OFFHAND,false);
        if accepted{state(|s|s.previous_source=Some(from));}accepted
    })
}

fn auto_inventory(game: &mut Game<'_>, m: &Module) -> bool {
    let inventory=alive(game) && game.screen == Screen::Inventory
        && Game::menu_slot_index(&game.remote_menu().1,0).is_some()
        && game.carried.is_none() && game.inv_drag.is_none();
    let can_open=plain(game);
    if !can_open && !inventory {
        // Native handled-screen opening can be unavailable while mining/sneaking.
        // An empty offhand may still use an existing hotbar totem.
        if alive(game) && game.screen==Screen::Playing && game.carried.is_none()
            && game.using.is_none() && game.offhand.is_none() {
            if let Some(from)=(0..9).find(|i|is_totem(&game.inventory[*i])){
                return schedule(game,m,INV,from,OFFHAND,true);
            }
        }
        return false;
    }
    let hotbar = slot_setting(m);
    if game.offhand.is_none() {
        let from = if !inventory && m.p("Mode") == "Hotbar Swap" {
            (0..9).find(|&i| is_totem(&game.inventory[i])).or_else(|| find_totem(game, 9))
        } else { find_totem(game, 9).or_else(|| find_totem(game, 0)) };
        if let Some(from) = from { return schedule(game, m, INV, from, OFFHAND, !inventory && m.p("Mode") == "Hotbar Swap" && from < 9); }
    }
    let to = if m.b("Hotbar refill") && !is_totem(&game.inventory[hotbar]) { Some(hotbar) } else { None };
    if let (Some(to), Some(from)) = (to, find_totem(game, 9)) { return schedule(game, m, INV, from, to, false); }
    false
}

fn double_hand(game: &mut Game<'_>, m: &Module) -> bool {
    if !m.b("Double Hand") {
        if let Some(slot) = state(|s| s.hand.restore(game.selected)) { hold(game, slot); }
        control::release(INV);
        return false;
    }
    let mut danger = m.b("Double Hand") && game.player.as_ref().is_some_and(|p|
        f64::from(p.health.health() + p.health.absorption()) <= 6.0);
    if m.b("Double Hand") && !danger {
        // Never inspect only a prefix and call an omitted blast safe. Every
        // observed crystal is checked, or the bounded overflow is dangerous.
        if game.crystals.len() > 512 || game.crystals.iter().any(|c|
            ![c.position.x,c.position.y,c.position.z].iter().all(|v|v.is_finite())) { danger = true; }
        else {
            let feet=game.player.as_ref().map(|p|[p.position.x,p.position.y,p.position.z]);
            let centers: Vec<_> = game.crystals.iter().map(|c| [c.position.x, c.position.y, c.position.z])
                .filter(|at|feet.is_some_and(|feet|super::sunset_explosive_policy::in_range(feet,*at,6.0))).take(17).collect();
            if centers.len()>16 {danger=true;}
            let mut budget = super::sunset_explosive_policy::Budget::new(16);
            for center in centers.into_iter().take(16) {
                let blast = super::sunset_explosive_damage::Blast::explosion(center, 6.0);
                let health = game.player.as_ref().map_or(0.0, |p| f64::from(p.health.health() + p.health.absorption()));
                if super::sunset_explosive_damage::local_damage(game, blast, &mut budget)
                    .is_none_or(|p| p.equipped_upper >= health) { danger = true; break; }
            }
        }
    }
    let main = is_totem(&game.inventory[game.selected]);
    let candidate = (0..9).find(|&i| i != game.selected && is_totem(&game.inventory[i]));
    let clean = plain(game) && !game.left && !game.right && is_totem(&game.offhand) && control::available(INV);
    if let Some(slot) = state(|s| s.hand.tick(danger, game.selected, main, candidate, clean)) {
        if control::claim(INV) { hold(game, slot); }
    }
    let holding = state(|s| s.hand.held.is_some());
    if holding { control::claim(INV); } else { control::release(INV); }
    holding
}

fn hover(game: &mut Game<'_>, m: &Module) -> bool {
    if game.screen != Screen::Inventory || game.carried.is_some() { return false; }
    let menu=game.remote_menu();
    let Some(from) = game.inventory_target().0.filter(|s| *s!=OFFHAND && *s!=200
        && Game::menu_slot_index(&menu.1,*s).is_some()) else { return false; };
    if !is_totem(&stack(game, from)) { return false; }
    let to = if !is_totem(&game.offhand) { OFFHAND }
        else if m.b("Hotbar refill") { slot_setting(m) } else { return false; };
    if is_totem(&stack(game, to)) { return false; }
    schedule(game, m, HOVER, from, to, false)
}

fn offhand(game: &mut Game<'_>, m: &Module, totem_priority: bool) -> bool {
    if !plain(game) { return false; }
    let Some(p) = game.player.as_ref() else { return false; };
    let health = f64::from(p.health.health() + p.health.absorption());
    let mut emergency = health <= m.n("Min health") || (m.b("Elytra") && p.fall_flying)
        || (m.b("Falling") && health - util::fall_damage(game) <= m.n("Min health"));
    if m.b("Explosion") && !emergency {
        if game.crystals.len() > 16 { emergency = true; }
        else {
            let centers: Vec<_> = game.crystals.iter().map(|c| [c.position.x,c.position.y,c.position.z]).collect();
            let mut budget = super::sunset_explosive_policy::Budget::new(16);
            for center in centers {
                let blast = super::sunset_explosive_damage::Blast::explosion(center, 6.0);
                if super::sunset_explosive_damage::local_damage(game,blast,&mut budget)
                    .is_none_or(|p|health-p.equipped_upper<=m.n("Min health")) { emergency=true; break; }
            }
        }
    }
    if totem_priority && !emergency { return false; }
    let main = game.inventory[game.selected].as_ref().map_or("", |s| util::short(&s.item));
    let sword = main.ends_with("_sword");
    let axe = main.ends_with("_axe");
    let clicking = game.right || game.physical_buttons.contains(&winit::event::MouseButton::Right);
    let item = if emergency { "Totem" }
        else if (m.b("Right gapple") && clicking && (!m.b("Sword gapple") || sword))
            || ((sword || axe) && m.b("Always gap on sword")) { "Enchanted gapple" }
        else if (m.b("Sword pot") && sword && clicking) || ((sword || axe) && m.b("Always pot on sword")) { "Potion" }
        else { m.p("Item") };
    let want = match item { "Totem" => "totem_of_undying", "Gapple" => "golden_apple",
        "Enchanted gapple" => "enchanted_golden_apple", "Shield" => "shield", "Potion" => "potion", _ => "end_crystal" };
    if game.offhand.as_ref().is_some_and(|s| util::short(&s.item) == want && s.count > 0) { return false; }
    let first = if emergency || m.b("Hotbar") { 0 } else { 9 };
    let from = (first..36).find(|&i| game.inventory[i].as_ref().is_some_and(|s| util::short(&s.item) == want && s.count > 0));
    from.is_some_and(|from| schedule(game, m, OFF, from, OFFHAND, false))
}

pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    let module = with(|s| s.module(THROW).cloned());
    let Some(m) = module else { return; };
    let pearl = sunset::key_from_text(m.t("Pearl key")).as_deref() == Some(key);
    let wind = sunset::key_from_text(m.t("Wind charge key")).as_deref() == Some(key);
    let edge = state(|s| {
        let a=pearl && s.pearl.update(pressed);let b=wind && s.wind.update(pressed);a||b
    });
    if !edge || !m.on || !alive(game) || game.screen!=Screen::Playing { return; }
    state(|s| s.press = Some(Press { kind: if pearl { Throw::Pearl } else { Throw::Wind }, slot: game.selected, tick:game.ticks }));
}

fn can_use(game: &Game<'_>, item: &ItemStack) -> bool {
    let Some(info) = game.res.items().get(&item.item) else { return false; };
    item.count > 0 && !game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(item, info.use_cooldown.as_ref()))
}

fn key_throw(game: &mut Game<'_>, m: &Module) -> bool {
    let Some(press) = state(|s| s.press) else { return false; };
    if game.selected!=press.slot || !policy::press_alive(press.tick,game.ticks) {
        state(|s|s.press=None);return false;
    }
    if !plain(game) || game.left || game.right {return false;}
    let want = match press.kind { Throw::Pearl => "ender_pearl", Throw::Wind => "wind_charge" };
    let slot=if game.offhand.as_ref().is_some_and(|s|util::short(&s.item)==want && s.count>0){Some(util::OFFHAND)}
        else{util::find_held(game, |s| s == want)};
    let Some(slot)=slot else{state(|s|s.press=None);return false;};
    let original = if slot == util::OFFHAND { game.offhand.clone() } else { game.inventory[slot].clone() };
    if !original.as_ref().is_some_and(|item| can_use(game, item)) || !control::claim(THROW) { return false; }
    let done = control::finish(THROW);
    // The actual native jump key is consumed by the next input tick. Never
    // invent an upward velocity or throw early from an unconsumed request.
    if matches!(press.kind,Throw::Wind) && game.player.as_ref().is_some_and(|p| p.ground.on_ground) {
        super::sunset_melee_assist::request_jump(THROW);
        state(|s| s.press = Some(press));
        return true;
    }
    let Some(connection) = session(game) else { return false; };
    let expected = m.clone();
    let (yaw, pitch) = (game.yaw, if matches!(press.kind, Throw::Wind) { 90.0 } else { game.pitch });
    let queued = Instant::now();
    state(|s| s.queued_use = Some((THROW, queued)));
    let accepted = sunset::try_turn_then(THROW, yaw, pitch, move |game| {
        let done = done;
        if !done.current() { return; }
        state(|s| s.queued_use = None);
        let actual = if slot == util::OFFHAND { game.offhand.as_ref() } else { game.inventory[slot].as_ref() };
        if queued.elapsed() > Duration::from_millis(200) || !same_session(game, &connection)
            || !plain(game) || game.left || game.right || game.selected != press.slot
            || !policy::press_alive(press.tick,game.ticks)
            || actual != original.as_ref() || !actual.is_some_and(|item| can_use(game,item))
            || !with(|s| s.module(THROW).is_some_and(|m| m.on && m == &expected)) { return; }
        let previous = game.selected;
        if slot != util::OFFHAND { hold(game, slot); }
        let sequence = util::sequence(game);
        game.send(S::UseItem(play::UseItem { hand: i32::from(slot == util::OFFHAND), sequence, yaw, pitch }));
        state(|s|s.press=None);
        if slot != util::OFFHAND && game.selected == slot { hold(game, previous); }
        state(|s| s.next = game.ticks.saturating_add(2));
    });
    if !accepted { state(|s| s.queued_use = None); }
    accepted
}

fn fast_xp(game: &mut Game<'_>, m: &Module) {
    if !plain(game) || game.left || !(game.right || game.physical_buttons.contains(&winit::event::MouseButton::Right))
        { return; }
    let main=game.inventory[game.selected].as_ref();
    let main_xp=main.is_some_and(|s|util::short(&s.item)=="experience_bottle");
    let main_block=main.is_some_and(|s|game.items().get(&s.item).is_some_and(|i|i.block.is_some()));
    let off_xp=game.offhand.as_ref().is_some_and(|s|util::short(&s.item)=="experience_bottle");
    if main_xp || (off_xp && !main_block) {
        if sunset::turn_pending() || !control::available(XP) || state(|s|s.last_action==Some(game.ticks)) {
            game.use_delay=game.use_delay.max(2);return;
        }
        // The actual native use path does cooldowns, hand choice and stack
        // prediction. Do not add a second UseItem beside the user's click.
        game.use_delay = game.use_delay.min(policy::delay(m.n("Delay"), 3) as u32 + 1);
    }
}

/// Root's existing before-native-use hook calls this after manual macro intent.
/// It consumes conflicting queued use operations without clearing attack/pick.
pub(super) fn intercept_use(game:&mut Game<'_>)->bool {
    if !plain(game) || !(game.right || game.physical_buttons.contains(&winit::event::MouseButton::Right))
        || !with(|s|s.module(XP).is_some_and(|m|m.on)){return false;}
    let main=game.inventory[game.selected].as_ref();
    let main_xp=main.is_some_and(|s|util::short(&s.item)=="experience_bottle");
    let main_block=main.is_some_and(|s|game.items().get(&s.item).is_some_and(|i|i.block.is_some()));
    let off_xp=game.offhand.as_ref().is_some_and(|s|util::short(&s.item)=="experience_bottle");
    let blocked=(main_xp || (off_xp && !main_block)) && (sunset::turn_pending()
        || !control::available(XP) || state(|s|s.last_action==Some(game.ticks)));
    if blocked {game.use_delay=game.use_delay.max(2);}blocked
}
/// Observe the actual final forwarding path, including replayed packets.
pub(super) fn forwarded(game:&Game<'_>,packet:&S) {
    if matches!(packet,S::Attack(_) | S::UseItem(_) | S::UseItemOn(_))
        && game.native_window() && game.native_on(sunset::ID) {
        state(|s|if s.connection.as_ref().is_some_and(|old|same_session(game,old)){s.last_action=Some(game.ticks);});
    }
}
/// Optional render/post-packet responsiveness, isolated from DoubleHand's
/// once-per-native-tick safe counter. Never drive another owner's job here.
pub(super) fn responsive(game:&mut Game<'_>,mods:&[Module]) {
    let Some(module)=mods.iter().find(|m|m.name==AUTO && m.on) else{return;};
    if !alive(game) || mods.iter().any(|m|m.name==INV && m.on){return;}
    let owner=state(|s|s.job.as_ref().map(|j|j.owner));
    if owner==Some(AUTO){run_job(game,mods);return;}
    if owner.is_some() || state(|s|s.queued_use.is_some()){return;}
    if state(|s|s.connection.as_ref().is_none_or(|old|!same_session(game,old))){
        reset();state(|s|s.connection=session(game));
    }
    auto_totem(game,module);
}

fn needs_repair(game: &Game<'_>, item: &Option<ItemStack>, threshold: f64) -> bool {
    item.as_ref().is_some_and(|s| s.enchantment_level("minecraft:mending") > 0
        && game.res.items().get(&s.item).is_some_and(|i| i.max_damage > 0
            && f64::from(i.max_damage.saturating_sub(s.damage)) / f64::from(i.max_damage) * 100.0 <= threshold))
}

fn auto_exp(game: &mut Game<'_>, m: &Module) -> bool {
    if !plain(game) || game.left || game.right || (m.b("Only on ground") && !game.player.as_ref().is_some_and(|p| p.ground.on_ground)) { return false; }
    let threshold = if state(|s| s.repairing) { m.n("Max threshold %") } else { m.n("Min threshold %") };
    let armor = m.p("Mode") != "Hands" && game.armor.iter().any(|s| needs_repair(game, s, threshold));
    let hands = m.p("Mode") != "Armor" && [&game.inventory[game.selected], &game.offhand].iter().any(|s| needs_repair(game, s, threshold));
    state(|s| s.repairing = armor || hands);
    if !armor && !hands { return false; }
    let Some(slot) = util::find_hotbar(game, |s| s == "experience_bottle") else {
        if m.b("Replenish") {
            if let Some(from) = util::find_inventory(game, |s| s == "experience_bottle") {
                let to = policy::delay(m.n("EXP slot"), 9).clamp(1, 9) as usize - 1;
                return schedule(game, m, REPAIR, from, to, false);
            }
        }
        return false;
    };
    if !game.inventory[slot].as_ref().is_some_and(|s| can_use(game, s)) || !control::claim(REPAIR) { return false; }
    let snapshot = m.clone();
    let original = game.inventory[slot].clone();
    let Some(connection) = session(game) else { control::release(REPAIR); return false; };
    let yaw = game.yaw;
    let selected = game.selected;
    let deadline = Instant::now();
    state(|s| s.queued_use = Some((REPAIR, deadline)));
    let accepted = sunset::try_turn_then(REPAIR, yaw, 90.0, move |game| {
        let valid = deadline.elapsed() <= Duration::from_millis(200) && same_session(game, &connection)
            && plain(game) && !game.left && !game.right && control::owns(REPAIR)
            && game.inventory[slot] == original && game.selected == selected
            && with(|s| s.module(REPAIR).is_some_and(|m| m.on && m == &snapshot));
        if valid {
            let before = game.selected;
            hold(game, slot);
            let sequence = util::sequence(game);
            game.send(S::UseItem(play::UseItem { hand: 0, sequence, yaw, pitch: 90.0 }));
            if game.selected == slot { hold(game, before); }
            state(|s| s.next = game.ticks.saturating_add(2));
        }
        state(|s| s.queued_use = None);
        control::release(REPAIR);
    });
    if !accepted { state(|s| s.queued_use = None); control::release(REPAIR); }
    accepted
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module], ticks: u64) {
    if !alive(game) { cancel(game); return; }
    let changed = state(|s| s.connection.as_ref().is_none_or(|old| !same_session(game, old)) || ticks < s.tick);
    if changed { reset(); state(|s| s.connection = session(game)); }
    state(|s| s.tick = ticks);
    let get = |name: &str| mods.iter().find(|m| m.name == name && m.on);
    let auto = get(AUTO);
    let inv = get(INV);
    if auto.is_none(){state(|s|{s.pace.clear();s.auto_retry.reset();});}
    // Last enabled wins, with deterministic Auto Totem precedence if both
    // arrive together from a saved profile. Never run two refill drivers.
    let disable = state(|s| {
        let which = if auto.is_some() && inv.is_some() {
            if !s.inv_on && s.auto_on { Some(AUTO) } else { Some(INV) }
        } else { None };
        s.auto_on = auto.is_some(); s.inv_on = inv.is_some(); which
    });
    if let Some(name) = disable { util::toggle_off(game, name); return; }
    if run_job(game, mods) { return; }
    if let Some((owner, at)) = state(|s| s.queued_use) {
        if at.elapsed() > Duration::from_millis(200) || get(owner).is_none() {
            sunset::cancel_turns(owner); state(|s| s.queued_use = None); control::release(owner);
        } else { control::claim(owner); return; }
    }
    if ticks < state(|s| s.next) { return; }
    if let Some(m) = auto { if auto_totem(game, m) { return; } }
    if let Some(m) = inv {
        if auto_inventory(game, m) { return; }
        if double_hand(game, m) { return; }
    } else {
        if let Some(slot) = state(|s| s.hand.restore(game.selected)) { hold(game, slot); }
        control::release(INV);
    }
    if let Some(m) = get(HOVER) { if hover(game, m) { return; } }
    let priority = inv.is_some() || auto.is_some();
    if let Some(m) = get(OFF) { if offhand(game, m, priority) { return; } }
    if let Some(m) = get(THROW) { if key_throw(game, m) { return; } } else { state(|s| s.press = None); }
    if let Some(m) = get(XP) { fast_xp(game, m); }
    if let Some(m) = get(REPAIR) { auto_exp(game, m); }
}
