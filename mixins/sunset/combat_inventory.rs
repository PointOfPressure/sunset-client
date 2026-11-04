// The inventory modules: Offhand (Meteor's Offhand for its items, vape-rs's Auto Totem for its totem, and Rust's Hover
// Totem from PR #16 as its Hover), Key Throw (#16) and Fast Use's Throwables (#16's FastXP). A Hover totem moves by one
// swap click, never by picking stacks up, and counts as moved once the server's own slot packets say both slots
// changed.
use super::sunset::{self, Category, Module, hold, with};
use super::sunset_combat_control as control;
use super::sunset_inventory_policy::{self as policy, Failure, OFFHAND, Status, Swap};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::items::ItemStack;
use horizon_protocol::packets::play::{self, Clientbound as C, Serverbound as S};
use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const INV: &str = "Offhand";
const HOVER: &str = "Offhand (hover)";
const XP: &str = "Fast Use";
const THROW: &str = "Key Throw";
const OWNERS: [&str; 4] = [INV, HOVER, XP, THROW];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(INV, Category::Combat, "Keeps a totem or another item in your offhand.")
            .pick("Item", &["Totem", "Crystal", "Gapple", "Enchanted gapple", "Shield", "Potion", "None"])
            .bool("Open inventory", true)
            .bool("Silent open", false)
            .num("Shortest silent move delay (ms)", 100.0, 50.0, 200.0, 1.0)
            .num("Longest silent move delay (ms)", 120.0, 50.0, 200.0, 1.0)
            .bool("Close inventory", true)
            .bool("Inventory only", false)
            .bool("Random slot", true)
            .num("Shortest totem delay (ms)", 100.0, 50.0, 200.0, 1.0)
            .num("Longest totem delay (ms)", 120.0, 50.0, 200.0, 1.0)
            .bool("Extra randomization", true)
            .bool("Show totem count", false)
            .num("Delay (ticks)", 0.0, 0.0, 20.0, 1.0)
            .bool("Hotbar", false)
            .bool("Right gapple", false)
            .bool("Sword gapple", false)
            .bool("Always gap on sword", false)
            .bool("Always pot on sword", false)
            .bool("Sword pot", false)
            .bool("Hover", false)
            .bool("Hotbar refill", false)
            .num("Hotbar slot", 1.0, 1.0, 9.0, 1.0)
            .bool("Double hand", false)
            .text("Double hand on", "anchors,crystals,obsidian,sword,mace")
            .num("Double hand sensitivity", 1.0, 0.5, 1.5, 0.05)
            .num("Reswap delay (ticks)", 1.0, 1.0, 10.0, 1.0),
        Module::new(THROW, Category::Combat, "Throws pearls and wind charges on keys, when hit, or after an enemy's pearl.")
            .text("Pearl key", "None")
            .text("Wind charge key", "None")
            .bool("Hotbar keys", false)
            .bool("Lunge spears", true)
            .text("Crossbow key", "None")
            .bool("Crossbow from inventory", true)
            .pick("Throw mode", &["Silent", "Legit"])
            .num("Shortest step delay (ms)", 10.0, 0.0, 200.0, 5.0)
            .num("Longest step delay (ms)", 30.0, 0.0, 200.0, 5.0)
            .num("Smoothing", 0.5, 0.1, 2.0, 0.1)
            .text("Short pearl key", "None")
            .num("Short pearl pitch", 70.0, 30.0, 90.0, 1.0)
            .num("Wind charge pitch", 90.0, 30.0, 90.0, 1.0)
            .bool("Wind charge along look", false)
            .bool("Wind charge after pearl", false)
            .bool("Auto short pearl", false)
            .num("Shortest hit delay (ms)", 30.0, 0.0, 300.0, 5.0)
            .num("Longest hit delay (ms)", 70.0, 0.0, 300.0, 5.0)
            .bool("Pearl follow", false)
            .num("Thrower range", 50.0, 5.0, 200.0, 1.0)
            .num("Min landing distance", 8.0, 0.0, 50.0, 1.0)
            .num("Landing range", 60.0, 5.0, 200.0, 1.0)
            .num("Max error", 2.0, 0.5, 10.0, 0.5)
            .num("Shortest follow delay (ms)", 50.0, 0.0, 500.0, 10.0)
            .num("Longest follow delay (ms)", 100.0, 0.0, 500.0, 10.0)
            .bool("Follow only on ground", false),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Swap,
    Await,
}

// One Hover move: from the swap click to the server's confirmation.
struct Job {
    lease: control::Finish,
    owner: &'static str,
    settings: Module,
    slots: [usize; 2],
    before: [Option<ItemStack>; 2],
    menu: (i32, String),
    wire: [Option<usize>; 2],
    phase: Phase,
    began: Instant,
    swap: Option<Swap<ItemStack>>,
    unsafe_ticks: u8,
    revision: Option<i32>,
}

// Auto Totem, as vape-rs's AutoTotem (Offhand's Item Totem): while the offhand holds no totem and the main inventory or
// hotbar has one, one swap-with-offhand click on it (the first, or a random one with Random slot) in your inventory:
// the one open, else one opened for it (Open inventory; closed again with Close inventory), or with Silent open the
// inventory clicked with no screen, the look the server has held and your move keys and clicks held until a Silent
// move delay after it ends. Inventory only waits for you to open it. Each step waits a Totem delay, and a click waits
// to be seen (the slot changes, or 1.5 s) before the next; one that changed nothing is not tried again on the same
// slots.
#[derive(Default)]
struct Totem {
    due: Option<Instant>,
    pending: Option<Pending>,
    stall: Option<Vec<Option<ItemStack>>>,
    opened: bool,
    silent: bool,
    suppress: bool,
    action_at: Option<Instant>,
    look: Option<(f32, f32)>,
    took: bool,
    seed: u64,
    work: Option<Work>,
}

// What a tick asks of Auto Totem. Its clicks and closes go out in the next frame, after the tick's end packet, as a
// click on a screen does (vape-rs's are sent there too); Grim's PacketOrderO flags one sent between the tick's move
// and its end, where the tick's modules run.
enum Work {
    Step(Module),
    WindDown(Module),
    Stop,
}

struct Pending {
    slot: usize,
    expected: Option<ItemStack>,
    revision: Option<i32>,
    menu: i32,
    deadline: Instant,
}

impl Totem {
    fn active(&self) -> bool {
        self.opened || self.silent || self.pending.is_some()
    }

    fn next(&mut self) -> u64 {
        if self.seed == 0 {
            self.seed = 0x70e7_d1ce;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    // The next step's wait in ms: even across the range, or with Extra randomization a normal spread round its middle
    // (a quarter of the range each way), kept in the range, and an 18 % chance of a longer pause on top.
    fn delay(&mut self, (min, max): (u64, u64), extra: bool) -> u64 {
        let (min, max) = (min.min(max) as f64, min.max(max) as f64);
        if !extra {
            return (min + (max - min) * self.unit()).round() as u64;
        }
        let range = (max - min).max(1.0);
        let sd = (range / 4.0).max(1.0);
        let (u1, u2) = (self.unit().max(f64::MIN_POSITIVE), self.unit());
        let gaussian = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        let mut wait = (min + range * 0.5 + gaussian * sd).clamp(min, max);
        if self.unit() < 0.18 {
            let (low, high) = ((range * 1.25).max(120.0), (range * 3.25).max(350.0));
            wait += low + (high - low) * self.unit();
        }
        wait.max(1.0).round() as u64
    }

    // Your keys stay held from the silent open until a Silent move delay (picked again each tick) after its last
    // click or tick.
    fn suppress_tick(&mut self, now: Instant, (min, max): (u64, u64)) -> bool {
        if self.suppress {
            let (low, high) = (min.min(max) as f64, min.max(max) as f64);
            let sample = (low + (high - low) * self.unit()) as u64;
            let elapsed = self.action_at.map_or(u64::MAX, |at| now.saturating_duration_since(at).as_millis() as u64);
            if elapsed >= sample {
                self.suppress = false;
            }
        }
        if self.silent {
            self.action_at = Some(now);
        }
        self.suppress
    }
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
    totem: Totem,
    // The tick a hit went out: a throw waits for the next one. (A use does not: the packets of a tick's use can go
    // on the wire as the next tick starts, and Throwables' next use is due then.)
    last_hit: Option<u64>,
    // Packets sent in a tick: (tick, count), for Burst's cap.
    sent: (u64, u32),
    burst: u32,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

// A module turned on or off: its move in progress stops (the slot goes back, the inventory it opened closes).
pub(super) fn toggled(game: &mut Game<'_>, name: &str) {
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
    state(|s| s.job.take_if(|j| j.settings.name == name));
    if name == INV {
        totem_stop(game);
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
        totem_stop(game);
    }
    post_tick(game);
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

fn slot_setting(m: &Module) -> usize {
    policy::delay(m.n("Hotbar slot"), 9).clamp(1, 9) as usize - 1
}

fn fail(game: &mut Game<'_>, job: Job) {
    state(|s| {
        s.failures.retain(|(owner, _)| *owner != job.owner);
        s.failures.push((job.owner, Failure { slots: job.slots, before: job.before }));
        s.next = game.ticks.saturating_add(4);
    });
}

// Starts moving `from` to `to` (the offhand or a hotbar slot) by one swap click.
fn schedule(game: &mut Game<'_>, m: &Module, owner: &'static str, from: usize, to: usize) -> bool {
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
    if wire[0].is_none() || (to != OFFHAND && wire[1].is_none()) || !control::claim(owner) {
        return false;
    }
    let lease = control::finish(owner);
    if !lease.current() {
        return false;
    }
    state(|s| {
        s.job = Some(Job {
            lease,
            owner,
            settings: m.clone(),
            slots: [from, to],
            before,
            menu,
            wire,
            phase: Phase::Swap,
            began: Instant::now(),
            swap: None,
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
        return true;
    }
    if !job.lease.current()
        || !alive(game)
        || game.remote_menu() != job.menu
        || game.carried.is_some()
        || (job.phase == Phase::Swap && game.inventory_target().0 != Some(job.slots[0]))
        // The server's 5 s at most to confirm, and a second more.
        || job.began.elapsed() > Duration::from_secs(6)
        || !control::claim(job.owner)
    {
        fail(game, job);
        return true;
    }
    match job.phase {
        Phase::Swap => {
            // A click in a menu while you move or sprint: a server may take it for cheating; wait up to 10 ticks.
            if !super::remote::sunset_combat_remote::quiet(game) {
                job.unsafe_ticks = job.unsafe_ticks.saturating_add(1);
                if job.unsafe_ticks > 10 {
                    fail(game, job);
                } else {
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
            let ([from, to], menu) = (job.slots, job.menu.clone());
            state(|s| s.job = Some(job));
            if !super::remote::sunset_combat_remote::swap_unpredicted(game, &menu, from, to)
                && let Some(job) = state(|s| s.job.take())
            {
                fail(game, job);
            }
        }
        Phase::Await => {
            let timeout = policy::confirmation_ticks(super::remote::sunset_combat_remote::latency(game));
            match job.swap.as_ref().map_or(Status::Rejected, |s| s.status(game.ticks, timeout)) {
                Status::Waiting => state(|s| s.job = Some(job)),
                Status::Confirmed => state(|s| s.next = game.ticks.saturating_add(1)),
                _ => fail(game, job),
            }
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
        let next = state(|s| s.job.as_ref().map(|j| j.phase));
        if next.is_none_or(|phase| phase == Phase::Await || Some(phase) == before) {
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

// Offhand: the item it wants by quick clicks, a totem by Auto Totem, and Hover.
fn offhand(game: &mut Game<'_>, m: &Module) -> bool {
    use super::sunset_combat::Want;
    let want = super::sunset_combat::offhand_want(game, m);
    let totem = matches!(want, Want::Totem);
    if !totem {
        state(|s| s.totem.work = Some(Work::WindDown(m.clone())));
    }
    if let Want::Item(item) = want {
        super::sunset_combat::offhand_click(game, m, item);
    }
    (totem && auto_totem(game, m)) || (m.b("Hover") && hover(game, m))
}

// The totems Show totem count puts beside Offhand in the module list: the main inventory's, the hotbar's and the
// offhand's.
static TOTEMS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub(super) fn totem_count() -> u32 {
    TOTEMS.load(Ordering::Relaxed)
}

fn count_totems(game: &Game<'_>) -> u32 {
    let totems = |s: &Option<ItemStack>| s.as_ref().filter(|s| &*s.item == "minecraft:totem_of_undying").map_or(0, |s| s.count);
    game.inventory.iter().take(36).map(totems).sum::<u32>() + totems(&game.offhand)
}

// The player's own inventory, shown: Auto Totem opens it as the game does.
fn open_inventory(game: &mut Game<'_>) {
    game.reset_inventory_clicks();
    game.screen = Screen::Inventory;
    game.capture(false);
}

fn close_inventory(game: &mut Game<'_>) {
    game.close_container();
    game.screen = Screen::Playing;
    game.capture(true);
}

// The slots and the carried stack: a click that changed nothing is not tried again while they stay so.
fn contents(game: &Game<'_>) -> Vec<Option<ItemStack>> {
    game.inventory.iter().take(36).cloned().chain([game.offhand.clone(), game.carried.clone()]).collect()
}

// The swap-with-offhand click (SWAP, button 40) on `slot`: the game's own click, predicted here and numbered with its
// menu state. Never on a locked slot.
fn swap_to_offhand(game: &mut Game<'_>, slot: usize) -> bool {
    !super::sunset_loadouts::locked(slot)
        && game.remote_menu_ops(&[(Some(slot), 40, horizon_client::menu::Input::Swap)])
}

// A silent session ends: the server's look goes back to yours, and the inventory it used is closed on the server
// (not when a container took its place: that container's close does it).
fn end_silent(game: &mut Game<'_>, t: &mut Totem, now: Instant, close: bool) {
    t.silent = false;
    t.look = None;
    t.action_at = Some(now);
    sunset::cancel_turns(INV);
    control::release(INV);
    if close && game.remote_menu().0 == 0 {
        game.close_container();
    }
}

// Equipped, no totems, or stalled: the inventory Auto Totem opened closes, a silent session ends.
fn wind_down(game: &mut Game<'_>, m: &Module, t: &mut Totem, now: Instant) -> bool {
    if t.silent {
        end_silent(game, t, now, true);
        return true;
    }
    let opened = std::mem::take(&mut t.opened);
    if opened && m.b("Close inventory") && game.screen == Screen::Inventory && game.remote_menu().0 == 0 {
        close_inventory(game);
        return true;
    }
    false
}

// One step; true when it did something (opened, clicked, closed).
fn totem_step(game: &mut Game<'_>, m: &Module, t: &mut Totem, now: Instant) -> bool {
    let container = game.remote_menu().0;
    let screen = game.screen == Screen::Inventory;
    // Another window: Auto Totem waits; a silent session ends there.
    if t.silent && container != 0 {
        t.pending = None;
        end_silent(game, t, now, false);
        return true;
    }
    if t.pending.as_ref().is_some_and(|p| p.menu != container) || (t.opened && !t.silent && !screen && t.pending.is_some()) {
        totem_cancel(game, t, now);
        return false;
    }
    if let Some(p) = &t.pending {
        if stack(game, p.slot) != p.expected || game.carried.is_some() {
            t.pending = None;
        } else if super::remote::sunset_combat_remote::container_state(game) != p.revision || now >= p.deadline {
            t.pending = None;
            t.stall = Some(contents(game));
        } else {
            return false;
        }
    }
    if container != 0 {
        return false;
    }
    if t.stall.as_ref().is_some_and(|c| *c == contents(game)) || is_totem(&game.offhand) {
        return wind_down(game, m, t, now);
    }
    let sources: Vec<usize> = (9..36).chain(0..9).filter(|&i| is_totem(&game.inventory[i]) && !super::sunset_loadouts::locked(i)).collect();
    if sources.is_empty() {
        return wind_down(game, m, t, now);
    }
    if !(screen || t.silent) {
        if m.b("Inventory only") || !m.b("Open inventory") {
            return false;
        }
        if m.b("Silent open") {
            // Another module has the hand or the look: not now.
            if !control::claim(INV) {
                return false;
            }
            (t.silent, t.suppress, t.action_at, t.look) = (true, true, Some(now), Some((game.yaw, game.pitch)));
            return true;
        }
        t.opened = true;
        open_inventory(game);
        return true;
    }
    if game.carried.is_some() {
        return false;
    }
    let source = if m.b("Random slot") { sources[(t.next() % sources.len() as u64) as usize] } else { sources[0] };
    let (expected, revision) = (stack(game, source), super::remote::sunset_combat_remote::container_state(game));
    if !swap_to_offhand(game, source) {
        return false;
    }
    t.pending = Some(Pending { slot: source, expected, revision, menu: container, deadline: now + Duration::from_millis(1500) });
    if t.silent {
        t.action_at = Some(now);
    }
    true
}

fn totem_cancel(game: &mut Game<'_>, t: &mut Totem, now: Instant) {
    t.pending = None;
    t.opened = false;
    if t.silent {
        end_silent(game, t, now, true);
    }
}

// Each tick with Item Totem: the next frame's step, and the look a silent session holds. True while a step is under
// way.
fn auto_totem(game: &mut Game<'_>, m: &Module) -> bool {
    TOTEMS.store(count_totems(game), Ordering::Relaxed);
    let (look, active) = state(|s| {
        s.totem.work = Some(Work::Step(m.clone()));
        (s.totem.look, s.totem.active())
    });
    if let Some((yaw, pitch)) = look {
        sunset::turn(INV, yaw, pitch);
    }
    active
}

// A step once its Totem delay is over.
fn totem_frame(game: &mut Game<'_>, m: &Module) {
    let now = Instant::now();
    let mut t = state(|s| std::mem::take(&mut s.totem));
    if t.due.is_none_or(|due| now >= due) {
        let acted = totem_step(game, m, &mut t, now);
        let range = (m.n("Shortest totem delay (ms)") as u64, m.n("Longest totem delay (ms)") as u64);
        let wait = t.delay(range, m.b("Extra randomization"));
        t.due = (acted || t.active()).then(|| now + Duration::from_millis(wait));
    }
    state(|s| s.totem = t);
}

// Each frame: what the last tick asked of Auto Totem, in the world it was asked in.
pub(super) fn frame(game: &mut Game<'_>) {
    let Some(work) = state(|s| s.totem.work.take()) else {
        return;
    };
    if !alive(game) || !state(|s| s.connection.as_ref().is_some_and(|old| same_session(game, old))) {
        return;
    }
    match work {
        Work::Step(m) => totem_frame(game, &m),
        Work::WindDown(m) => totem_wind_down(game, &m),
        Work::Stop => totem_stop(game),
    }
}

// Another Item, or Offhand off: what Auto Totem opened closes.
fn totem_wind_down(game: &mut Game<'_>, m: &Module) {
    let mut t = state(|s| std::mem::take(&mut s.totem));
    if t.active() {
        t.pending = None;
        wind_down(game, m, &mut t, Instant::now());
    }
    t.due = None;
    state(|s| s.totem = t);
}

fn totem_stop(game: &mut Game<'_>) {
    let mut t = state(|s| std::mem::take(&mut s.totem));
    if t.opened && game.screen == Screen::Inventory && game.remote_menu().0 == 0 && game.carried.is_none() {
        close_inventory(game);
    }
    totem_cancel(game, &mut t, Instant::now());
    state(|s| s.totem = Totem { seed: t.seed, took: t.took, ..Totem::default() });
}

// Before the game's tick: Silent open's hold on your move keys and clicks.
pub(super) fn pre_tick(game: &mut Game<'_>, m: Option<&Module>) {
    let held = state(|s| match m {
        Some(m) => {
            let range = (m.n("Shortest silent move delay (ms)") as u64, m.n("Longest silent move delay (ms)") as u64);
            s.totem.suppress_tick(Instant::now(), range)
        }
        None => std::mem::take(&mut s.totem.suppress),
    });
    if held && m.is_some() {
        for k in HELD_KEYS {
            game.keys.remove(&k);
        }
        (game.left, game.right) = (false, false);
        state(|s| s.totem.took = true);
    }
}

const HELD_KEYS: [winit::keyboard::KeyCode; 6] = {
    use winit::keyboard::KeyCode::*;
    [KeyW, KeyA, KeyS, KeyD, Space, ControlLeft]
};

// After the game's tick: the keys and clicks held go back to what you hold.
pub(super) fn post_tick(game: &mut Game<'_>) {
    if state(|s| std::mem::take(&mut s.totem.took)) {
        give_back(game);
    }
}

fn give_back(game: &mut Game<'_>) {
    use winit::event::MouseButton;
    for k in HELD_KEYS {
        if game.physical_keys.contains(&k) {
            game.keys.insert(k);
        }
    }
    game.left = game.physical_buttons.contains(&MouseButton::Left);
    game.right = game.physical_buttons.contains(&MouseButton::Right);
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
    !is_totem(&stack(game, to)) && schedule(game, m, HOVER, from, to)
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
    if m.p("Throw mode") == "Legit" {
        let item = if pearl { super::sunset_throws::Item::Pearl } else { super::sunset_throws::Item::Wind };
        super::sunset_throws::legit_press(game, &m, item);
        return;
    }
    state(|s| s.press = Some(Press { kind: if pearl { Throw::Pearl } else { Throw::Wind }, slot: game.selected, tick: game.ticks, from: None }));
}

// Hotbar keys (Duelkit's Quick use): the hotbar key of a slot with an ender pearl or wind charge throws it, and the
// slot you held stays (the game selects the slot on the key; the throw puts it back first).
fn hotbar_key(game: &mut Game<'_>, key: &str) {
    let Some(from) = (0..9).find(|i| game.settings.keys.get(&format!("key.hotbar.{}", i + 1)) == key) else {
        return;
    };
    if from == game.selected || game.screen != Screen::Playing || !alive(game) {
        return;
    }
    let kind = match game.inventory[from].as_ref().map(|s| util::short(&s.item)) {
        Some("ender_pearl") => Throw::Pearl,
        Some("wind_charge") => Throw::Wind,
        _ => {
            if with(|s| s.module(THROW).is_some_and(|m| m.b("Lunge spears")))
                && game.inventory[from].as_ref().is_some_and(|s| s.enchantment_level("minecraft:lunge") > 0)
            {
                super::sunset_melee::quick_lunge(game, from);
            }
            return;
        }
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
    let down = press.from.is_none() && matches!(press.kind, Throw::Wind) && !m.b("Wind charge along look");
    if down && game.player.as_ref().is_some_and(|p| p.ground.on_ground) {
        super::sunset_melee::request_jump(THROW);
        return true;
    }
    let Some(connection) = session(game) else {
        return false;
    };
    let expected = m.clone();
    let (yaw, pitch) = (game.yaw, if down { m.n("Wind charge pitch") as f32 } else { game.pitch });
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
        if matches!(press.kind, Throw::Pearl) {
            super::sunset_throws::pearl_thrown(game);
        }
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

// What Fast Use's Throwables throw faster: an experience bottle, ender pearl, wind charge, snowball or egg, or splash
// or lingering potion in hand as its settings say, off cooldown (a use on cooldown still sends its packet); with Off
// hand, one in the offhand while the hand is empty or holds a weapon (the game tries the main hand first).
pub(super) fn thrown_in_hand(game: &Game<'_>, m: &Module) -> bool {
    if !m.b("Throwables") {
        return false;
    }
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

// Throwables: the game's own use, with its use delay cut to Throw delay (Delay) or one tick (Every tick, Burst); it
// waits while a look is on its way or another module has the hand. (A use this tick was the game's own: the next one
// is a tick away, and the use hook holds it back if something else goes out first.)
fn fast_xp(game: &mut Game<'_>, m: &Module) {
    if !plain(game) || game.left || !holding_use(game) || !thrown_in_hand(game, m) {
        return;
    }
    if sunset::turn_pending() || !control::available(XP) {
        game.use_delay = game.use_delay.max(2);
        return;
    }
    game.use_delay = match m.p("Throw mode") {
        "Delay" => game.use_delay.min(m.n("Throw delay") as u32),
        _ => 1,
    };
}

// The use hook: Throwables hold back a use the tick it must wait.
pub(super) fn intercept_use(game: &mut Game<'_>) -> bool {
    if !plain(game) || !holding_use(game) || !with(|s| s.module(XP).is_some_and(|m| m.on && thrown_in_hand(game, m))) {
        return false;
    }
    let blocked = sunset::turn_pending() || !control::available(XP) || state(|s| s.last_hit == Some(game.ticks));
    if blocked {
        game.use_delay = game.use_delay.max(2);
    }
    blocked
}

// Burst: the uses the game's own one brings with it this tick, each as one more click would be (so before the
// tick's move packet), while the tick has sent fewer than BURST_CAP packets. A use aimed at a block is three (the use
// on it, the use, the swing), so a tick sends at most 7 + 3 + the game's own 3 + its move and end: 15, 300 a second,
// well under the 500 a second Paper allows. Only throwables without a cooldown.
static BURSTING: AtomicBool = AtomicBool::new(false);
const BURST_CAP: u32 = 8;

pub(super) fn bursting() -> bool {
    BURSTING.load(Ordering::Relaxed)
}

pub(super) fn burst_more(game: &Game<'_>) -> bool {
    let Some(m) = with(|s| s.module(XP).filter(|m| m.on && m.p("Throw mode") == "Burst").cloned()) else {
        return false;
    };
    let item = game.held().filter(|s| s.count > 1).map(|s| util::short(&s.item).to_owned());
    let free = item.is_some_and(|i| matches!(i.as_str(), "experience_bottle" | "snowball" | "egg" | "blue_egg" | "brown_egg" | "splash_potion" | "lingering_potion"));
    let more = free
        && plain(game)
        && thrown_in_hand(game, &m)
        && state(|s| {
            let sent = if s.sent.0 == game.ticks { s.sent.1 } else { 0 };
            if !bursting() {
                s.burst = 0;
            }
            s.burst += 1;
            s.burst < m.n("Burst") as u32 && sent < BURST_CAP
        });
    BURSTING.store(more, Ordering::Relaxed);
    more
}

// What went on the wire: hits, and every packet for Burst's count.
pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    let hit = matches!(packet, S::Attack(_));
    state(|s| {
        if !s.connection.as_ref().is_some_and(|old| same_session(game, old)) {
            return;
        }
        if hit {
            s.last_hit = Some(game.ticks);
        }
        s.sent = if s.sent.0 == game.ticks { (game.ticks, s.sent.1 + 1) } else { (game.ticks, 1) };
    });
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
        if offhand(game, m) {
            return;
        }
    } else if state(|s| s.totem.active()) {
        state(|s| s.totem.work = Some(Work::Stop));
    }
    match get(THROW) {
        Some(m) => {
            if key_throw(game, m) {
                return;
            }
        }
        None => state(|s| s.press = None),
    }
    if let Some(m) = get(XP).filter(|m| m.b("Throwables")) {
        fast_xp(game, m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totem_delays_stay_in_range_with_an_occasional_long_pause() {
        let mut t = Totem::default();
        let even: Vec<_> = (0..200).map(|_| t.delay((100, 120), false)).collect();
        assert!(even.iter().all(|d| (100..=120).contains(d)));
        let spread: Vec<_> = (0..400).map(|_| t.delay((100, 120), true)).collect();
        assert!(spread.iter().all(|d| *d >= 100 && *d <= 120 + 350));
        assert!(spread.iter().any(|d| *d > 200), "occasional long pause");
        assert!(spread.iter().filter(|d| **d <= 120).count() > 250);
        // A random slot reaches every one of them.
        let picked: std::collections::BTreeSet<_> = (0..32).map(|_| t.next() % 2).collect();
        assert_eq!(picked.len(), 2);
    }

    #[test]
    fn silent_open_holds_keys_until_a_move_delay_after_the_session() {
        let start = Instant::now();
        let mut t = Totem { silent: true, suppress: true, action_at: Some(start), ..Totem::default() };
        // A session renews its time each tick (50 ms apart): held all through it.
        for tick in 0..=8 {
            assert!(t.suppress_tick(start + Duration::from_millis(tick * 50), (150, 150)), "held all session");
        }
        let end = start + Duration::from_millis(400);
        t.silent = false;
        t.action_at = Some(end);
        assert!(t.suppress_tick(end + Duration::from_millis(140), (150, 150)));
        assert!(!t.suppress_tick(end + Duration::from_millis(150), (150, 150)));
        assert!(!t.suppress_tick(end + Duration::from_millis(160), (150, 150)), "released for good");
    }
}
