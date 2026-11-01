// The inventory modules from Rust's PR #16: Auto Inventory Totem, Hover Totem, FastXP and Key Throw. A totem moves
// by one swap click (or the offhand swap key from the hotbar), never by picking stacks up, and counts as moved once
// the server's own slot packets say both slots changed.
use super::sunset::{self, Category, Module, hold, with};
use super::sunset_combat_control as control;
use super::sunset_inventory_policy::{self as policy, Failure, OFFHAND, Status, Swap};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::items::ItemStack;
use horizon_protocol::packets::play::{self, Clientbound as C, Serverbound as S};
use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const INV: &str = "Auto Inventory Totem";
const HOVER: &str = "Hover Totem";
const XP: &str = "FastXP";
const THROW: &str = "Key Throw";
const OWNERS: [&str; 4] = [INV, HOVER, XP, THROW];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(INV, Category::Combat, "Moves a totem to your offhand as soon as it is empty: Inventory opens your inventory for the swap, Hotbar Swap takes one from the hotbar with the swap key. Hotbar refill keeps a totem in a hotbar slot; Double Hand holds one in your hand too while health is low or a crystal near you could kill. Turns Auto Totem off.")
            .pick("Mode", &["Inventory", "Hotbar Swap"])
            .bool("Hotbar refill", false)
            .num("Hotbar slot", 1.0, 1.0, 9.0, 1.0)
            .num("Reaction delay", 0.0, 0.0, 10.0, 1.0)
            .num("Swap delay", 1.0, 0.0, 10.0, 1.0)
            .num("Close delay", 0.0, 0.0, 10.0, 1.0)
            .num("Switch back delay", 2.0, 0.0, 10.0, 1.0)
            .bool("Double Hand", false),
        Module::new(HOVER, Category::Combat, "In your inventory, moves the totem under the pointer into your offhand (or the Hotbar slot, with Hotbar refill).")
            .bool("Hotbar refill", false)
            .num("Hotbar slot", 1.0, 1.0, 9.0, 1.0),
        Module::new(XP, Category::Combat, "Throws experience bottles faster while you hold use with one: the game's use delay becomes Delay. Also pearls, wind charges and the rest as ticked (Duelkit's Fast throw): thrown the tick their cooldown ends. Off hand: from the offhand while your hand is empty or holds a weapon.")
            .num("Delay", 0.0, 0.0, 3.0, 1.0)
            .bool("Experience bottles", true)
            .bool("Ender pearls", true)
            .bool("Wind charges", true)
            .bool("Snowballs and eggs", false)
            .bool("Splash potions", false)
            .bool("Off hand", true),
        Module::new(THROW, Category::Combat, "A key throws one ender pearl or wind charge from your hotbar (a wind charge straight down, jumping) and goes back to your slot. Hotbar keys: the hotbar key of a pearl or wind charge throws it where you look and keeps your weapon in hand.")
            .text("Pearl key", "None")
            .text("Wind charge key", "None")
            .bool("Hotbar keys", false),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Reaction,
    SwapDelay,
    Await,
    Close,
    Return,
}

// One totem move: from the reaction delay to the server's confirmation and the inventory closed again.
struct Job {
    lease: control::Finish,
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
    revision: Option<i32>,
}

#[derive(Clone, Copy)]
enum Throw {
    Pearl,
    Wind,
}

#[derive(Clone, Copy)]
struct Press {
    kind: Throw,
    slot: usize,
    tick: u64,
    // Hotbar keys: the slot whose key was pressed, thrown with your own look.
    from: Option<usize>,
}

type Session = (i32, super::Dim, Arc<dyn Any + Send + Sync>);

#[derive(Default)]
struct State {
    connection: Option<Session>,
    tick: u64,
    job: Option<Job>,
    failures: Vec<(&'static str, Failure<ItemStack>)>,
    pearl: policy::Edge,
    wind: policy::Edge,
    press: Option<Press>,
    next: u64,
    queued_use: Option<(&'static str, Instant)>,
    hand: policy::Hand,
    last_action: Option<u64>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

// A module turned on or off: its move in progress stops (the slot goes back, the inventory it opened closes). Auto
// Totem and Auto Inventory Totem never run together.
pub(super) fn toggled(game: &mut Game<'_>, name: &str, on: bool) {
    if name == THROW {
        super::sunset_melee::cancel_owner(game, THROW);
        state(|s| {
            s.press = None;
            s.pearl = policy::Edge::default();
            s.wind = policy::Edge::default();
        });
        if state(|s| s.queued_use.is_some_and(|(owner, _)| owner == THROW)) {
            sunset::cancel_turns(THROW);
            control::release(THROW);
            state(|s| s.queued_use = None);
        }
    }
    if let Some(job) = state(|s| s.job.take_if(|j| j.settings.name == name)) {
        restore(game, &job);
    }
    if on && name == "Auto Totem" {
        util::toggle_off(game, INV);
    }
    if on && name == INV {
        util::toggle_off(game, "Auto Totem");
    }
}

fn reset() {
    for owner in OWNERS {
        sunset::cancel_turns(owner);
        control::release(owner);
    }
    state(|s| *s = State::default());
}

// Sunset off or gone: what is in progress is undone in this world, then everything dropped.
pub(super) fn cancel(game: &mut Game<'_>) {
    if state(|s| s.connection.as_ref().is_some_and(|old| same_session(game, old))) {
        if let Some(job) = state(|s| s.job.take()) {
            restore(game, &job);
        }
        if let Some(slot) = state(|s| s.hand.restore(game.selected)) {
            hold(game, slot);
        }
    }
    reset();
}

fn session(game: &Game<'_>) -> Option<Session> {
    let r = game.remote.as_ref()?;
    Some((r.entity_id, game.dim, r.session.counters.clone()))
}

fn same_session(game: &Game<'_>, old: &Session) -> bool {
    game.remote.as_ref().is_some_and(|r| {
        r.entity_id == old.0 && game.dim == old.1 && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ()
    })
}

fn alive(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && game.remote.is_some()
        && !game.spectator()
        && !game.creative
        && !sunset::gui_open()
        && matches!(game.screen, Screen::Playing | Screen::Inventory)
        && game.selected < 9
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
}

// Playing with nothing else going on: no menu, nothing carried, used or mined.
fn plain(game: &Game<'_>) -> bool {
    alive(game)
        && game.screen == Screen::Playing
        && game.remote_menu().0 == 0
        && game.carried.is_none()
        && game.using.is_none()
        && game.mining.is_none()
        && super::remote::sunset_remote::breaking(game).is_none()
}

fn stack(game: &mut Game<'_>, slot: usize) -> Option<ItemStack> {
    game.slot_mut(slot).clone()
}

fn is_totem(stack: &Option<ItemStack>) -> bool {
    stack.as_ref().is_some_and(|s| &*s.item == "minecraft:totem_of_undying" && s.count == 1)
}

fn find_totem(game: &Game<'_>, from: usize) -> Option<usize> {
    (from..36).find(|&i| is_totem(&game.inventory[i]))
}

fn slot_setting(m: &Module) -> usize {
    policy::delay(m.n("Hotbar slot"), 9).clamp(1, 9) as usize - 1
}

// The slot a hotbar move took goes back, and the inventory the job opened closes (never a container, never with
// something carried).
fn restore(game: &mut Game<'_>, job: &Job) {
    if !job.lease.current() {
        return;
    }
    if let Some((before, ours)) = job.restore
        && game.selected == ours
        && before < 9
    {
        hold(game, before);
    }
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
}

// Starts moving `from` to `to` (the offhand or a hotbar slot); `hotbar`: by the swap key from the hotbar.
fn schedule(game: &mut Game<'_>, m: &Module, owner: &'static str, from: usize, to: usize, hotbar: bool) -> bool {
    if from == to || !(to < 9 || to == OFFHAND) || game.carried.is_some() {
        return false;
    }
    let before = [stack(game, from), stack(game, to)];
    if !before.iter().flatten().all(|s| game.res.items().get(&s.item).is_some_and(|info| s.count > 0 && s.count <= info.max_stack)) {
        return false;
    }
    if before[0].is_none()
        || before[0] == before[1]
        || state(|s| s.failures.iter().any(|(o, f)| *o == owner && f.blocks([from, to], &before)))
    {
        return false;
    }
    let menu = game.remote_menu();
    let wire = [Game::menu_slot_index(&menu.1, from), Game::menu_slot_index(&menu.1, to)];
    if wire[0].is_none() || (!hotbar && to != OFFHAND && wire[1].is_none()) || !control::claim(owner) {
        return false;
    }
    let lease = control::finish(owner);
    if !lease.current() {
        return false;
    }
    let reaction = if owner == INV { policy::delay(m.n("Reaction delay"), 10) } else { 0 };
    state(|s| {
        s.job = Some(Job {
            lease,
            owner,
            settings: m.clone(),
            slots: [from, to],
            before,
            menu,
            wire,
            phase: Phase::Reaction,
            due: game.ticks.saturating_add(reaction),
            began: Instant::now(),
            swap: None,
            hotbar,
            opened: false,
            restore: None,
            pass_swaps: 0,
            unsafe_ticks: 0,
            revision: None,
        })
    });
    run_job(game, std::slice::from_ref(m));
    true
}

// One step of the move in progress. False: there is none.
fn run_job_step(game: &mut Game<'_>, mods: &[Module]) -> bool {
    let Some(mut job) = state(|s| s.job.take()) else {
        return false;
    };
    if !mods.iter().any(|m| m.on && m == &job.settings) {
        restore(game, &job);
        return true;
    }
    if !job.lease.current()
        || !alive(game)
        || game.remote_menu() != job.menu
        || game.carried.is_some()
        || job.restore.is_some_and(|(_, ours)| game.selected != ours)
        || (job.owner == HOVER && matches!(job.phase, Phase::Reaction | Phase::SwapDelay) && game.inventory_target().0 != Some(job.slots[0]))
        // The server's 5 s at most to confirm, and a second for the delays.
        || job.began.elapsed() > Duration::from_secs(6)
        || !control::claim(job.owner)
    {
        fail(game, job);
        return true;
    }
    if game.ticks < job.due {
        state(|s| s.job = Some(job));
        return true;
    }
    match job.phase {
        Phase::Reaction => {
            if job.owner == INV && !job.hotbar && game.screen == Screen::Playing {
                game.reset_inventory_clicks();
                game.screen = Screen::Inventory;
                game.capture(false);
                job.opened = true;
            }
            if job.hotbar {
                let from = job.slots[0];
                if from >= 9 {
                    fail(game, job);
                    return true;
                }
                job.restore = Some((game.selected, from));
                hold(game, from);
            }
            job.phase = Phase::SwapDelay;
            let wait = if job.owner == INV { policy::delay(job.settings.n("Swap delay"), 10).max(u64::from(job.opened)) } else { 0 };
            job.due = game.ticks.saturating_add(wait);
            state(|s| s.job = Some(job));
        }
        Phase::SwapDelay => {
            // A click in a menu while you move or sprint: a server may take it for cheating; wait up to 10 ticks.
            if !job.hotbar && !super::remote::sunset_combat_remote::quiet(game) {
                job.unsafe_ticks = job.unsafe_ticks.saturating_add(1);
                if job.unsafe_ticks > 10 {
                    fail(game, job);
                } else {
                    job.due = game.ticks.saturating_add(1);
                    state(|s| s.job = Some(job));
                }
                return true;
            }
            if [stack(game, job.slots[0]), stack(game, job.slots[1])] != job.before {
                fail(game, job);
                return true;
            }
            job.swap = Swap::new(job.slots, job.before.clone(), game.ticks);
            if job.swap.is_none() {
                fail(game, job);
                return true;
            }
            job.phase = Phase::Await;
            job.began = Instant::now();
            job.revision = super::remote::sunset_combat_remote::container_state(game);
            let ([from, to], hotbar, menu) = (job.slots, job.hotbar, job.menu.clone());
            if hotbar && (from >= 9 || to != OFFHAND) {
                fail(game, job);
                return true;
            }
            state(|s| s.job = Some(job));
            let sent = if hotbar {
                // The swap-with-offhand key (PlayerAction 7) for the slot in hand.
                game.send(S::PlayerAction(play::PlayerAction { action: 7, pos: [0; 3], direction: 0, sequence: 0 }));
                true
            } else {
                super::remote::sunset_combat_remote::swap_unpredicted(game, &menu, from, to)
            };
            if !sent && let Some(job) = state(|s| s.job.take()) {
                fail(game, job);
            }
        }
        Phase::Await => {
            let timeout = policy::confirmation_ticks(super::remote::sunset_combat_remote::latency(game));
            match job.swap.as_ref().map_or(Status::Rejected, |s| s.status(game.ticks, timeout)) {
                Status::Waiting => state(|s| s.job = Some(job)),
                Status::Confirmed => {
                    job.pass_swaps = job.pass_swaps.saturating_add(1);
                    // Then Hotbar refill, once in the same pass.
                    let to = slot_setting(&job.settings);
                    let follow = if job.owner == INV
                        && !job.hotbar
                        && job.pass_swaps < 2
                        && job.settings.b("Hotbar refill")
                        && !is_totem(&game.inventory[to])
                    {
                        find_totem(game, 9).map(|from| (from, to))
                    } else {
                        None
                    };
                    if let Some((from, to)) = follow {
                        job.slots = [from, to];
                        job.before = [stack(game, from), stack(game, to)];
                        job.wire = [Game::menu_slot_index(&job.menu.1, from), Game::menu_slot_index(&job.menu.1, to)];
                        job.swap = None;
                        job.phase = Phase::SwapDelay;
                        job.due = game.ticks.saturating_add(policy::delay(job.settings.n("Swap delay"), 10).max(1));
                    } else {
                        job.phase = if job.hotbar { Phase::Return } else { Phase::Close };
                        let wait = if job.owner == INV {
                            policy::delay(job.settings.n(if job.hotbar { "Switch back delay" } else { "Close delay" }), 10)
                        } else {
                            0
                        };
                        job.due = game.ticks.saturating_add(wait);
                    }
                    state(|s| s.job = Some(job));
                }
                _ => fail(game, job),
            }
        }
        Phase::Close | Phase::Return => {
            restore(game, &job);
            state(|s| s.next = game.ticks.saturating_add(if job.owner == INV && !job.hotbar { 4 } else { 1 }));
        }
    }
    true
}

// The steps already due this tick (at most 4); waiting for the server never spins.
fn run_job(game: &mut Game<'_>, mods: &[Module]) -> bool {
    let mut used = false;
    for _ in 0..4 {
        let before = state(|s| s.job.as_ref().map(|j| j.phase));
        if !run_job_step(game, mods) {
            break;
        }
        used = true;
        let next = state(|s| s.job.as_ref().map(|j| (j.phase, j.due)));
        if next.is_none_or(|(phase, due)| phase == Phase::Await || Some(phase) == before || due > game.ticks) {
            break;
        }
    }
    used
}

// The server's packets (copies, in order): what they say the swapped slots hold.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if matches!(packet, C::Login(_) | C::Respawn(_) | C::Disconnect(_)) {
        let dropped = state(std::mem::take);
        if let Some((owner, _)) = dropped.queued_use {
            sunset::cancel_turns(owner);
        }
        return;
    }
    state(|s| {
        if !s.connection.as_ref().is_some_and(|old| same_session(game, old)) {
            return;
        }
        let Some(job) = s.job.as_mut().filter(|j| j.phase == Phase::Await) else {
            return;
        };
        let revision = match packet {
            C::ContainerSetSlot(p) if p.container == job.menu.0 => Some(p.state),
            C::ContainerSetContent(p) if p.container == job.menu.0 => Some(p.state),
            _ => None,
        };
        if let Some(revision) = revision {
            if job.revision.is_some_and(|previous| !policy::revision_current(previous, revision)) {
                return;
            }
            job.revision = Some(revision);
        }
        let Some(swap) = job.swap.as_mut() else {
            return;
        };
        match packet {
            C::ContainerSetSlot(p) if p.slot >= 0 => {
                for i in 0..2 {
                    if (p.container == job.menu.0 && job.wire[i] == Some(p.slot as usize))
                        || (p.container == 0 && policy::player_wire(job.slots[i]) == Some(p.slot as usize))
                    {
                        swap.observe(job.slots[i], &game.to_stack(&p.item));
                    }
                }
            }
            C::ContainerSetContent(p) => {
                for i in 0..2 {
                    let wire = if p.container == job.menu.0 {
                        job.wire[i]
                    } else if p.container == 0 {
                        policy::player_wire(job.slots[i])
                    } else {
                        None
                    };
                    if let Some(item) = wire.and_then(|wire| p.items.get(wire)) {
                        swap.observe(job.slots[i], &game.to_stack(item));
                    }
                }
                if p.carried.is_some() {
                    swap.reject();
                }
            }
            C::SetPlayerInventory(p) => {
                if let Some(slot) = policy::player_internal(p.slot) {
                    swap.observe(slot, &game.to_stack(&p.item));
                }
            }
            C::SetCursorItem(p) if p.item.is_some() => swap.reject(),
            C::OpenScreen(_) => swap.reject(),
            _ => {}
        }
    });
}

fn auto_inventory(game: &mut Game<'_>, m: &Module) -> bool {
    let inventory = alive(game)
        && game.screen == Screen::Inventory
        && Game::menu_slot_index(&game.remote_menu().1, 0).is_some()
        && game.carried.is_none()
        && game.inv_drag.is_none();
    if !plain(game) && !inventory {
        // The inventory cannot open now (mining, using): a hotbar totem can still go by the swap key.
        if alive(game) && game.screen == Screen::Playing && game.carried.is_none() && game.using.is_none() && game.offhand.is_none()
            && let Some(from) = (0..9).find(|i| is_totem(&game.inventory[*i]))
        {
            return schedule(game, m, INV, from, OFFHAND, true);
        }
        return false;
    }
    if game.offhand.is_none() {
        let by_key = !inventory && m.p("Mode") == "Hotbar Swap";
        let from = if by_key {
            (0..9).find(|&i| is_totem(&game.inventory[i])).or_else(|| find_totem(game, 9))
        } else {
            find_totem(game, 9).or_else(|| find_totem(game, 0))
        };
        if let Some(from) = from {
            return schedule(game, m, INV, from, OFFHAND, by_key && from < 9);
        }
    }
    let hotbar = slot_setting(m);
    if m.b("Hotbar refill") && !is_totem(&game.inventory[hotbar]) && let Some(from) = find_totem(game, 9) {
        return schedule(game, m, INV, from, hotbar, false);
    }
    false
}

// Health and absorption at 6 or less, or a crystal within 6 blocks that could take all of it (each checked, at most
// 16; more counts as danger).
fn in_danger(game: &mut Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let health = f64::from(p.health.health() + p.health.absorption());
    let feet = [p.position.x, p.position.y, p.position.z];
    if health <= 6.0 || game.crystals.len() > 512 {
        return true;
    }
    let centers: Vec<_> = game
        .crystals
        .iter()
        .map(|c| [c.position.x, c.position.y, c.position.z])
        .filter(|at| !at.iter().all(|v| v.is_finite()) || super::sunset_explosive_policy::in_range(feet, *at, 6.0))
        .take(17)
        .collect();
    if centers.len() > 16 || centers.iter().any(|at| !at.iter().all(|v| v.is_finite())) {
        return true;
    }
    let mut budget = super::sunset_explosive_policy::Budget::new(16);
    centers.into_iter().any(|center| {
        let blast = super::sunset_explosive_damage::Blast::explosion(center, 6.0);
        super::sunset_explosive_damage::local_damage(game, blast, &mut budget).is_none_or(|p| p.estimate >= health)
    })
}

fn double_hand(game: &mut Game<'_>, m: &Module) -> bool {
    if !m.b("Double Hand") {
        if let Some(slot) = state(|s| s.hand.restore(game.selected)) {
            hold(game, slot);
        }
        control::release(INV);
        return false;
    }
    let danger = in_danger(game);
    let main = is_totem(&game.inventory[game.selected]);
    let candidate = (0..9).find(|&i| i != game.selected && is_totem(&game.inventory[i]));
    let clean = plain(game) && !game.left && !game.right && is_totem(&game.offhand) && control::available(INV);
    if let Some(slot) = state(|s| s.hand.tick(danger, game.selected, main, candidate, clean))
        && control::claim(INV)
    {
        hold(game, slot);
    }
    let holding = state(|s| s.hand.held.is_some());
    if holding {
        control::claim(INV);
    } else {
        control::release(INV);
    }
    holding
}

fn hover(game: &mut Game<'_>, m: &Module) -> bool {
    if game.screen != Screen::Inventory || game.carried.is_some() {
        return false;
    }
    let menu = game.remote_menu();
    let Some(from) = game.inventory_target().0.filter(|s| *s != OFFHAND && *s != 200 && Game::menu_slot_index(&menu.1, *s).is_some())
    else {
        return false;
    };
    if !is_totem(&stack(game, from)) {
        return false;
    }
    let to = if !is_totem(&game.offhand) {
        OFFHAND
    } else if m.b("Hotbar refill") {
        slot_setting(m)
    } else {
        return false;
    };
    !is_totem(&stack(game, to)) && schedule(game, m, HOVER, from, to, false)
}

// Key Throw's keys: a press (not a held key) asks for one throw.
pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    let Some(m) = with(|s| s.module(THROW).cloned()) else {
        return;
    };
    let pearl = sunset::key_from_text(m.t("Pearl key")).as_deref() == Some(key);
    let wind = sunset::key_from_text(m.t("Wind charge key")).as_deref() == Some(key);
    let edge = state(|s| {
        let a = pearl && s.pearl.update(pressed);
        let b = wind && s.wind.update(pressed);
        a || b
    });
    if pressed && m.on && m.b("Hotbar keys") {
        hotbar_key(game, key);
    }
    if !edge || !m.on || !alive(game) || game.screen != Screen::Playing {
        return;
    }
    state(|s| s.press = Some(Press { kind: if pearl { Throw::Pearl } else { Throw::Wind }, slot: game.selected, tick: game.ticks, from: None }));
}

// Hotbar keys (Duelkit's Quick use): the hotbar key of a slot with an ender pearl or wind charge throws it, and the
// slot you held stays (the game selects the slot on the key; the throw puts it back first).
fn hotbar_key(game: &Game<'_>, key: &str) {
    let Some(from) = (0..9).find(|i| game.settings.keys.get(&format!("key.hotbar.{}", i + 1)) == key) else {
        return;
    };
    if from == game.selected || game.screen != Screen::Playing || !alive(game) {
        return;
    }
    let kind = match game.inventory[from].as_ref().map(|s| util::short(&s.item)) {
        Some("ender_pearl") => Throw::Pearl,
        Some("wind_charge") => Throw::Wind,
        _ => return,
    };
    state(|s| s.press = Some(Press { kind, slot: game.selected, tick: game.ticks, from: Some(from) }));
}

fn can_use(game: &Game<'_>, item: &ItemStack) -> bool {
    let Some(info) = game.res.items().get(&item.item) else {
        return false;
    };
    item.count > 0 && !game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(item, info.use_cooldown.as_ref()))
}

// The throw: a wind charge on the ground jumps first; then, with the look the server has (straight down for a wind
// charge), the slot switched, one UseItem, and the slot back.
fn key_throw(game: &mut Game<'_>, m: &Module) -> bool {
    let Some(press) = state(|s| s.press) else {
        return false;
    };
    if press.from == Some(game.selected) {
        game.selected = press.slot;
    }
    if game.selected != press.slot || !policy::press_alive(press.tick, game.ticks) {
        state(|s| s.press = None);
        return false;
    }
    if !plain(game) || game.left || game.right {
        return false;
    }
    let want = match press.kind {
        Throw::Pearl => "ender_pearl",
        Throw::Wind => "wind_charge",
    };
    let slot = if press.from.is_some() {
        press.from
    } else if game.offhand.as_ref().is_some_and(|s| util::short(&s.item) == want && s.count > 0) {
        Some(util::OFFHAND)
    } else {
        util::find_held(game, |s| s == want)
    };
    let Some(slot) = slot else {
        state(|s| s.press = None);
        return false;
    };
    let original = if slot == util::OFFHAND { game.offhand.clone() } else { game.inventory[slot].clone() };
    if !original.as_ref().is_some_and(|item| can_use(game, item)) || !control::claim(THROW) {
        return false;
    }
    let done = control::finish(THROW);
    let down = press.from.is_none() && matches!(press.kind, Throw::Wind);
    if down && game.player.as_ref().is_some_and(|p| p.ground.on_ground) {
        super::sunset_melee::request_jump(THROW);
        return true;
    }
    let Some(connection) = session(game) else {
        return false;
    };
    let expected = m.clone();
    let (yaw, pitch) = (game.yaw, if down { 90.0 } else { game.pitch });
    let queued = Instant::now();
    state(|s| s.queued_use = Some((THROW, queued)));
    let accepted = sunset::try_turn_then(THROW, yaw, pitch, move |game| {
        let done = done;
        if !done.current() {
            return;
        }
        state(|s| s.queued_use = None);
        let actual = if slot == util::OFFHAND { game.offhand.as_ref() } else { game.inventory[slot].as_ref() };
        if queued.elapsed() > Duration::from_millis(200)
            || !same_session(game, &connection)
            || !plain(game)
            || game.left
            || game.right
            || game.selected != press.slot
            || !policy::press_alive(press.tick, game.ticks)
            || actual != original.as_ref()
            || !actual.is_some_and(|item| can_use(game, item))
            || !with(|s| s.module(THROW).is_some_and(|m| m.on && m == &expected))
        {
            return;
        }
        let previous = game.selected;
        if slot != util::OFFHAND {
            hold(game, slot);
        }
        let sequence = util::sequence(game);
        game.send(S::UseItem(play::UseItem { hand: i32::from(slot == util::OFFHAND), sequence, yaw, pitch }));
        state(|s| s.press = None);
        if slot != util::OFFHAND && game.selected == slot {
            hold(game, previous);
        }
        state(|s| s.next = game.ticks.saturating_add(2));
    });
    if !accepted {
        state(|s| s.queued_use = None);
    }
    accepted
}

// What FastXP throws faster: an experience bottle, ender pearl, wind charge, snowball or egg, or splash or lingering
// potion in hand as its settings say, off cooldown (a use on cooldown still sends its packet); with Off hand, one in
// the offhand while the hand is empty or holds a weapon (the game tries the main hand first).
fn thrown_in_hand(game: &Game<'_>, m: &Module) -> bool {
    let wanted = |s: &ItemStack| {
        let ticked = match util::short(&s.item) {
            "experience_bottle" => m.b("Experience bottles"),
            "ender_pearl" => m.b("Ender pearls"),
            "wind_charge" => m.b("Wind charges"),
            "snowball" | "egg" | "blue_egg" | "brown_egg" => m.b("Snowballs and eggs"),
            "splash_potion" | "lingering_potion" => m.b("Splash potions"),
            _ => false,
        };
        ticked && can_use(game, s)
    };
    let main = game.inventory[game.selected].as_ref();
    let weapon = main.is_none_or(|s| ["_sword", "_axe", "_spear", ":mace", ":trident"].iter().any(|w| s.item.ends_with(w)));
    main.is_some_and(wanted) || (m.b("Off hand") && weapon && game.offhand.as_ref().is_some_and(wanted))
}

fn holding_use(game: &Game<'_>) -> bool {
    game.right || game.physical_buttons.contains(&winit::event::MouseButton::Right)
}

// FastXP: the game's own use, with its use delay cut to Delay; it waits while a look is on its way, another module
// has the hand, or something went out this tick.
fn fast_xp(game: &mut Game<'_>, m: &Module) {
    if !plain(game) || game.left || !holding_use(game) || !thrown_in_hand(game, m) {
        return;
    }
    if sunset::turn_pending() || !control::available(XP) || state(|s| s.last_action == Some(game.ticks)) {
        game.use_delay = game.use_delay.max(2);
        return;
    }
    game.use_delay = game.use_delay.min(policy::delay(m.n("Delay"), 3) as u32 + 1);
}

// The use hook: FastXP holds back a use the tick it must wait.
pub(super) fn intercept_use(game: &mut Game<'_>) -> bool {
    if !plain(game) || !holding_use(game) || !with(|s| s.module(XP).is_some_and(|m| m.on && thrown_in_hand(game, m))) {
        return false;
    }
    let blocked = sunset::turn_pending() || !control::available(XP) || state(|s| s.last_action == Some(game.ticks));
    if blocked {
        game.use_delay = game.use_delay.max(2);
    }
    blocked
}

// What went on the wire: a hit or a use counts as the tick's action.
pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    if matches!(packet, S::Attack(_) | S::UseItem(_) | S::UseItemOn(_)) {
        state(|s| {
            if s.connection.as_ref().is_some_and(|old| same_session(game, old)) {
                s.last_action = Some(game.ticks);
            }
        });
    }
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    if !alive(game) {
        cancel(game);
        return;
    }
    let ticks = game.ticks;
    if state(|s| s.connection.as_ref().is_none_or(|old| !same_session(game, old)) || ticks < s.tick) {
        reset();
        state(|s| s.connection = session(game));
    }
    state(|s| s.tick = ticks);
    let get = |name: &str| mods.iter().find(|m| m.name == name && m.on);
    if run_job(game, mods) {
        return;
    }
    if let Some((owner, at)) = state(|s| s.queued_use) {
        if at.elapsed() > Duration::from_millis(200) || get(owner).is_none() {
            sunset::cancel_turns(owner);
            state(|s| s.queued_use = None);
            control::release(owner);
        } else {
            control::claim(owner);
            return;
        }
    }
    if ticks < state(|s| s.next) {
        return;
    }
    if let Some(m) = get(INV) {
        if auto_inventory(game, m) || double_hand(game, m) {
            return;
        }
    } else {
        if let Some(slot) = state(|s| s.hand.restore(game.selected)) {
            hold(game, slot);
        }
        control::release(INV);
    }
    if let Some(m) = get(HOVER)
        && hover(game, m)
    {
        return;
    }
    match get(THROW) {
        Some(m) => {
            if key_throw(game, m) {
                return;
            }
        }
        None => state(|s| s.press = None),
    }
    if let Some(m) = get(XP) {
        fast_xp(game, m);
    }
}
