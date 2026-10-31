// Original inventory utilities built on Horizon's predicted menu engine and
// authoritative incoming slot packets. No carried-stack pickup sequences.
use super::{Game, Screen};
use super::sunset::{self, Category, Module, hold, with};
use super::sunset_combat_control as control;
use super::sunset_inventory_policy::{self as policy, Failure, Status, Swap, OFFHAND};
use super::sunset_util as util;
use horizon_client::{items::ItemStack, menu::Input};
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
            .num("Reaction delay", 0.0, 0.0, 20.0, 1.0)
            .num("Swap delay", 1.0, 0.0, 10.0, 1.0)
            .num("Close delay", 0.0, 0.0, 10.0, 1.0)
            .num("Switch back delay", 2.0, 0.0, 10.0, 1.0)
            .bool("Double Hand", false).num("Double hand health", 6.0, 1.0, 36.0, 1.0),
        Module::new(HOVER, Category::Combat, "Moves only the totem under your inventory pointer into the offhand or configured hotbar slot. Waits for both server slot updates.")
            .bool("Hotbar refill", false).num("Hotbar slot", 1.0, 1.0, 9.0, 1.0)
            .num("Delay (ticks)", 2.0, 0.0, 20.0, 1.0),
        Module::new(XP, Category::Combat, "Shortens the native item-use delay only while you hold use with an experience bottle. Yields to other combat actions.")
            .num("Delay (ticks)", 0.0, 0.0, 3.0, 1.0),
        Module::new(THROW, Category::Combat, "Throws one pearl or wind charge per key press and returns the selected slot. Never repeats a held key.")
            .text("Pearl key", "key.keyboard.g").text("Wind charge key", "key.keyboard.v"),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase { Reaction, SwapDelay, Await, Finish }

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
}

#[derive(Clone, Copy)]
enum Throw { Pearl, Wind }
#[derive(Clone, Copy)]
struct Press { kind: Throw, slot: usize, at: Instant }

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
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

pub(super) fn debug() -> serde_json::Value {
    state(|s| serde_json::json!({
        "owner":s.job.as_ref().map(|j|j.owner),
        "phase":s.job.as_ref().map(|j|match j.phase { Phase::Reaction=>"reaction",Phase::SwapDelay=>"swap-delay",Phase::Await=>"server-ack",Phase::Finish=>"finish" }),
        "slots":s.job.as_ref().map(|j|j.slots),
        "ack_bits":s.job.as_ref().and_then(|j|j.swap.as_ref()).map(|p|p.confirmations()),
        "failed_owners":s.failures.len(),
        "double_hand":s.hand.held,
        "queued_throw":s.press.is_some(),
        "queued_use":s.queued_use.map(|p|p.0),
    }))
}

pub(super) fn reset() {
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
    restore(game, &job);
    state(|s| {
        s.failures.retain(|(owner, _)| *owner != job.owner);
        s.failures.push((job.owner, Failure { slots: job.slots, before: job.before }));
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
    let reaction = if owner == INV { policy::delay(m.n("Reaction delay"), 20) }
        else if owner == AUTO { policy::delay(m.n("Refill delay"), 10) }
        else if owner == HOVER { policy::delay(m.n("Delay (ticks)"), 20) }
        else { policy::delay(m.n("Item switch delay"), 20) };
    state(|s| s.job = Some(Job { owner, settings: m.clone(), slots: [from, to], before, menu, wire,
        phase: Phase::Reaction, due: game.ticks.saturating_add(reaction), began: Instant::now(),
        swap: None, hotbar, opened: false, restore: None }));
    true
}

fn run_job(game: &mut Game<'_>, mods: &[Module]) -> bool {
    let Some(mut job) = state(|s| s.job.take()) else { return false; };
    if !alive(game) || !mods.iter().any(|m| m.on && m == &job.settings)
        || game.remote_menu() != job.menu || game.carried.is_some()
        || (job.owner == HOVER && matches!(job.phase, Phase::Reaction | Phase::SwapDelay)
            && game.inventory_target().0 != Some(job.slots[0]))
        || job.began.elapsed() > Duration::from_secs(4) || !control::claim(job.owner) {
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
            job.phase = Phase::SwapDelay;
            job.due = game.ticks.saturating_add(if job.owner == INV { policy::delay(job.settings.n("Swap delay"), 10) } else { 0 });
            state(|s| s.job = Some(job));
        }
        Phase::SwapDelay => {
            if [stack(game, job.slots[0]), stack(game, job.slots[1])] != job.before {
                fail(game, job); return true;
            }
            job.swap = Swap::new(job.slots, job.before.clone(), game.ticks);
            if job.swap.is_none() { fail(game, job); return true; }
            job.phase = Phase::Await;
            if job.hotbar {
                let before = game.selected;
                let from = job.slots[0];
                if from >= 9 || job.slots[1] != OFFHAND { fail(game, job); return true; }
                hold(game, from);
                job.restore = Some((before, from));
                state(|s| s.job = Some(job));
                game.send(S::PlayerAction(play::PlayerAction { action: 6, pos: [0; 3], direction: 0, sequence: 0 }));
            } else {
                let from = job.slots[0];
                let button = if job.slots[1] == OFFHAND { 40 } else { job.slots[1] as i32 };
                state(|s| s.job = Some(job));
                if !game.remote_menu_ops(&[(Some(from), button, Input::Swap)]) {
                    if let Some(job) = state(|s| s.job.take()) { fail(game, job); }
                }
            }
        }
        Phase::Await => {
            match job.swap.as_ref().map_or(Status::Rejected, |s| s.status(game.ticks, 30)) {
                Status::Waiting => state(|s| s.job = Some(job)),
                Status::Confirmed => {
                    job.phase = Phase::Finish;
                    let close = if job.owner == INV { policy::delay(job.settings.n("Close delay"), 10)
                        .max(policy::delay(job.settings.n("Switch back delay"), 10)) } else { 0 };
                    job.due = game.ticks.saturating_add(close);
                    state(|s| s.job = Some(job));
                }
                _ => fail(game, job),
            }
        }
        Phase::Finish => {
            restore(game, &job);
            state(|s| s.next = game.ticks.saturating_add(1));
            control::release(job.owner);
        }
    }
    true
}

// Called at HEAD of the native packet handler. Values come from server packet
// payloads, not the optimistically predicted inventory already on the client.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    state(|s| {
        if !s.connection.as_ref().is_some_and(|old| same_session(game, old)) { return; }
        let Some(job) = s.job.as_mut().filter(|j| j.phase == Phase::Await) else { return; };
        let Some(swap) = job.swap.as_mut() else { return; };
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
    });
}

fn auto_totem(game: &mut Game<'_>, m: &Module) -> bool {
    if !plain(game) || game.offhand.is_some() { return false; }
    let first = if m.b("Use hotbar totems") { 0 } else { 9 };
    find_totem(game, first).is_some_and(|from| schedule(game, m, AUTO, from, OFFHAND, false))
}

fn auto_inventory(game: &mut Game<'_>, m: &Module) -> bool {
    if !plain(game) && !(alive(game) && game.screen == Screen::Inventory
        && game.remote_storage_open() && game.carried.is_none() && game.inv_drag.is_none()) { return false; }
    let hotbar = slot_setting(m);
    if game.offhand.is_none() {
        let from = if m.p("Mode") == "Hotbar Swap" {
            (0..9).find(|&i| is_totem(&game.inventory[i])).or_else(|| find_totem(game, 9))
        } else { find_totem(game, 9).or_else(|| find_totem(game, 0)) };
        if let Some(from) = from { return schedule(game, m, INV, from, OFFHAND, m.p("Mode") == "Hotbar Swap" && from < 9); }
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
        f64::from(p.health.health() + p.health.absorption()) <= m.n("Double hand health"));
    if m.b("Double Hand") && !danger {
        let at = util::player_pos(game).unwrap_or([f64::NAN; 3]);
        let centers: Vec<_> = game.crystals.iter().take(16).map(|c| [c.position.x, c.position.y, c.position.z])
            .filter(|p| super::sunset_explosive_policy::in_range(at, *p, 6.0)).collect();
        let mut budget = super::sunset_explosive_policy::Budget::new(16);
        for center in centers {
            let blast = super::sunset_explosive_damage::Blast { center, power: 6.0, ignored_block: None };
            let health = game.player.as_ref().map_or(0.0, |p| f64::from(p.health.health() + p.health.absorption()));
            if super::sunset_explosive_damage::local_damage(game, blast, &mut budget)
                .is_none_or(|p| p.equipped_upper >= health) { danger = true; break; }
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
    let Some(from) = game.inventory_target().0.filter(|s| *s < 36 || (100..109).contains(s) || (300..354).contains(s)) else { return false; };
    if !is_totem(&stack(game, from)) { return false; }
    let to = if !is_totem(&game.offhand) { OFFHAND }
        else if m.b("Hotbar refill") { slot_setting(m) } else { return false; };
    if is_totem(&stack(game, to)) { return false; }
    schedule(game, m, HOVER, from, to, false)
}

fn offhand(game: &mut Game<'_>, m: &Module, totem_priority: bool) -> bool {
    if !plain(game) || totem_priority { return false; }
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
                let blast = super::sunset_explosive_damage::Blast { center,power:6.0,ignored_block:None };
                if super::sunset_explosive_damage::local_damage(game,blast,&mut budget)
                    .is_none_or(|p|health-p.equipped_upper<=m.n("Min health")) { emergency=true; break; }
            }
        }
    }
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
    let edge = state(|s| (pearl && s.pearl.update(pressed)) || (wind && s.wind.update(pressed)));
    if !edge || !m.on || !plain(game) || game.left || game.right { return; }
    state(|s| s.press = Some(Press { kind: if pearl { Throw::Pearl } else { Throw::Wind }, slot: game.selected, at: Instant::now() }));
}

fn can_use(game: &Game<'_>, item: &ItemStack) -> bool {
    let Some(info) = game.res.items().get(&item.item) else { return false; };
    item.count > 0 && !game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(item, info.use_cooldown.as_ref()))
}

fn key_throw(game: &mut Game<'_>, m: &Module) -> bool {
    let Some(press) = state(|s| s.press.take()) else { return false; };
    if !plain(game) || game.left || game.right || game.selected != press.slot || press.at.elapsed() > Duration::from_millis(250) { return false; }
    let want = match press.kind { Throw::Pearl => "ender_pearl", Throw::Wind => "wind_charge" };
    let slot = util::find_held(game, |s| s == want);
    let Some(slot) = slot else { return false; };
    let item = if slot == util::OFFHAND { game.offhand.as_ref() } else { game.inventory[slot].as_ref() };
    if !item.is_some_and(|item| can_use(game, item)) || !control::claim(THROW) { return false; }
    let before = game.selected;
    if slot != util::OFFHAND { hold(game, slot); }
    let sequence = util::sequence(game);
    game.send(S::UseItem(play::UseItem { hand: i32::from(slot == util::OFFHAND), sequence, yaw: game.yaw, pitch: game.pitch }));
    if slot != util::OFFHAND && game.selected == slot { hold(game, before); }
    state(|s| s.next = game.ticks.saturating_add(2));
    control::release(THROW);
    let _ = m;
    true
}

fn fast_xp(game: &mut Game<'_>, m: &Module) {
    if !plain(game) || game.left || !(game.right || game.physical_buttons.contains(&winit::event::MouseButton::Right))
        || !control::available(XP) { return; }
    if util::find_held(game, |s| s == "experience_bottle").is_some_and(|slot| slot == game.selected || slot == util::OFFHAND) {
        // The actual native use path does cooldowns, hand choice and stack
        // prediction. Do not add a second UseItem beside the user's click.
        game.use_delay = game.use_delay.min(policy::delay(m.n("Delay (ticks)"), 3) as u32 + 1);
    }
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
