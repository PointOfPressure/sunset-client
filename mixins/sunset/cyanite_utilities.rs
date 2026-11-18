// Independently written native utilities for the Cyanite feature comparison.
// Each session owns its timers, observations and pending work.
use super::Game;
use super::sunset::{Camera, Canvas, Category, Module, say};
use super::sunset_util::{center_of, dist, listed, short};
use super::sunset_cyanite_logic::crossed;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Depth Alert", Category::Player, "Alerts when you cross a chosen height.")
            .num("Y level", -2.0, -64.0, 320.0, 1.0)
            .pick("Trigger", &["Going down", "Going up", "Either way"]),
        Module::new("Auto Trident", Category::Movement, "Releases a charged Riptide trident while you hold use.")
            .num("Charge ticks", 10.0, 10.0, 40.0, 1.0),
        Module::new("Staff List", Category::Player, "Lists online names matching your staff list or tab ranks.")
            .text("Names", "").text("Ranks", "admin,moderator,mod,staff")
            .bool("Notify", true),
        Module::new("Base Guard", Category::Player, "Alerts or disconnects when an unfamiliar player comes near.")
            .num("Range", 64.0, 4.0, 256.0, 4.0)
            .num("Start after (s)", 5.0, 0.0, 120.0, 1.0)
            .text("Ignore", "").bool("Ignore friends", true).bool("Log out", false),
        Module::new("Relog", Category::Client, "Leaves the current multiplayer server and rejoins it once.")
            .num("Delay (s)", 1.0, 0.0, 30.0, 0.5),
        Module::new("Chest Index", Category::World, "Remembers containers you open and highlights matching items.")
            .text("Search", "").bool("Look in shulkers", true)
            .num("Range", 128.0, 8.0, 512.0, 8.0)
            .num("Max results", 16.0, 1.0, 64.0, 1.0)
            .colour("Color", "Cyan"),
    ]
}

#[derive(Clone, Serialize, Deserialize)]
struct Container {
    pos: [i32; 3],
    items: BTreeMap<String, u32>,
}

struct Retry {
    address: String,
    name: String,
    lan: bool,
    due: Instant,
}

struct State {
    world: Option<(u64, &'static str)>,
    last_y: Option<f64>,
    staff: HashSet<String>,
    guarded: HashSet<u32>,
    guard_started: Option<Instant>,
    retry: Option<Retry>,
    menu: i32,
    opened: Option<[i32; 3]>,
    containers: Vec<Container>,
}

impl Default for State {
    fn default() -> Self {
        Self { world: None, last_y: None, staff: HashSet::new(), guarded: HashSet::new(),
            guard_started: None, retry: None, menu: 0, opened: None, containers: Vec::new() }
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

fn module<'a>(mods: &'a [Module], name: &str) -> Option<&'a Module> {
    mods.iter().find(|m| m.name == name && m.on)
}

pub(super) fn world_key(game: &Game<'_>) -> Option<u64> {
    let remote = game.remote.as_ref()?;
    let identity = match remote.save_folder.as_deref() {
        Some(folder) => format!("local:{folder}"),
        None => format!("server:{}", remote.session.address),
    };
    Some(identity.bytes().fold(0xcbf2_9ce4_8422_2325_u64,
        |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)))
}

fn index_path(game: &Game<'_>) -> Option<std::path::PathBuf> {
    Some(game.native_data_dir(super::sunset::ID).join("chest-index")
        .join(format!("{:016x}-{}.json", world_key(game)?, short(game.dim.id()))))
}

fn relog(game: &mut Game<'_>, enabled: bool, delay: f64) {
    if !enabled {
        state(|s| s.retry = None);
        return;
    }
    if state(|s| s.retry.is_some()) {
        if game.remote.is_some() {
            state(|s| s.retry = None);
            let _ = super::sunset_api::toggle(game, "Relog", false);
            return;
        }
        if state(|s| s.retry.as_ref().is_some_and(|r| Instant::now() >= r.due)) {
            let retry = state(|s| s.retry.take()).unwrap();
            let result = game.connect_server_with_options(&retry.address, &retry.name, None, retry.lan, false);
            let _ = super::sunset_api::toggle(game, "Relog", false);
            if let Err(error) = result { say(game, &format!("Relog failed: {error}")); }
        }
        return;
    }
    let target = game.remote.as_ref().filter(|_| game.integrated.is_none() && !game.realm_session)
        .map(|r| (r.session.address.clone(), r.session.name.clone(), game.server_is_lan()));
    let Some((address, name, lan)) = target else {
        let _ = super::sunset_api::toggle(game, "Relog", false);
        say(game, "Relog needs an active multiplayer connection.");
        return;
    };
    if let Err(error) = game.leave_server(Some("Rejoining the server.".into())) {
        let _ = super::sunset_api::toggle(game, "Relog", false);
        say(game, &format!("Relog could not leave: {error}"));
        return;
    }
    state(|s| s.retry = Some(Retry { address, name, lan, due: Instant::now() + Duration::from_secs_f64(delay) }));
}

fn chest_index(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        state(|s| { s.menu = 0; s.opened = None; });
        return;
    };
    let menu = game.remote_menu().0;
    if menu == 0 {
        state(|s| { s.menu = 0; s.opened = None; });
        return;
    }
    if state(|s| s.menu != menu) {
        let at = game.pick().map(|h| h.pos).filter(|p| {
            let name = &game.res.facts().get(game.world.block(p[0], p[1], p[2])).name;
            listed("chest,trapped_chest,barrel,*shulker_box,hopper,dispenser,dropper,furnace,blast_furnace,smoker", name)
        });
        state(|s| { s.menu = menu; s.opened = at; });
    }
    let Some(pos) = state(|s| s.opened) else { return };
    if game.ticks % 10 != 0 { return; }
    let size = game.remote_chest_menu().map(|m| m.0 * 9)
        .or_else(|| (game.remote_menu().1 == "minecraft:shulker_box").then_some(27));
    let Some(size) = size else { return };
    let mut items = BTreeMap::new();
    for i in 0..size {
        if let Some(stack) = game.slot_mut(300 + i).as_ref() {
            *items.entry(stack.item.to_string()).or_insert(0) += stack.count;
            if m.b("Look in shulkers") {
                for (_, item) in stack.extra.iter().flat_map(|extra| &extra.container) {
                    *items.entry(item.item.to_string()).or_insert(0) += item.count;
                }
            }
        }
    }
    let changed = state(|s| {
        if let Some(container) = s.containers.iter_mut().find(|c| c.pos == pos) {
            if container.items == items { return false; }
            container.items = items;
        } else {
            if s.containers.len() >= 4096 { s.containers.remove(0); }
            s.containers.push(Container { pos, items });
        }
        true
    });
    if changed && let Some(path) = index_path(game) {
        let text = state(|s| serde_json::to_vec(&s.containers)).unwrap_or_default();
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(path.parent().unwrap())?;
            let temp = path.with_extension("tmp");
            std::fs::write(&temp, text)?;
            std::fs::rename(temp, path)
        })();
        if let Err(error) = result {
            say(game, &format!("Chest Index could not save: {error}"));
        }
    }
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    if game.native_playing() && !super::sunset::gui_open()
        && let Some(m) = module(mods, "Auto Trident")
    {
        super::remote::sunset_remote::release_charged_trident(game, m.n("Charge ticks") as u32);
    }
    let retry = module(mods, "Relog");
    relog(game, retry.is_some(), retry.map_or(1.0, |m| m.n("Delay (s)")));
    let world = world_key(game).map(|w| (w, game.dim.id()));
    if state(|s| s.world != world) {
        let containers = index_path(game).and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Vec<Container>>(&b).ok())
            .map(|mut c| { c.truncate(4096); c }).unwrap_or_default();
        state(|s| {
            let retry = s.retry.take();
            *s = State { world, containers, retry, ..State::default() };
        });
    }
    if let Some(player) = &game.player {
        let y = player.position.y;
        let before = state(|s| std::mem::replace(&mut s.last_y, Some(y)));
        if let Some(m) = module(mods, "Depth Alert")
            && let Some(before) = before
            && crossed(before, y, m.n("Y level"), m.p("Trigger"))
        {
            say(game, &format!("Depth Alert: crossed Y {} at {:.1}.", m.n("Y level"), y));
        }
    }
    let Some(m) = module(mods, "Staff List") else {
        state(|s| s.staff.clear());
        base_guard(game, module(mods, "Base Guard"));
        chest_index(game, module(mods, "Chest Index"));
        return;
    };
    let names: HashSet<String> = super::remote::sunset_remote::staff_names(game, m.t("Names"), m.t("Ranks")).into_iter().collect();
    let added = state(|s| {
        let added: Vec<_> = names.difference(&s.staff).cloned().collect();
        s.staff = names;
        added
    });
    if m.b("Notify") && !added.is_empty() {
        say(game, &format!("Staff online: {}", added.join(", ")));
    }
    base_guard(game, module(mods, "Base Guard"));
    chest_index(game, module(mods, "Chest Index"));
}

fn base_guard(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        state(|s| { s.guarded.clear(); s.guard_started = None; });
        return;
    };
    if !game.native_playing() || game.player.is_none() { return; }
    let ready = state(|s| s.guard_started.get_or_insert_with(Instant::now).elapsed().as_secs_f64() >= m.n("Start after (s)"));
    if !ready { return; }
    let players: Vec<_> = super::sunset_api::players(game).into_iter()
        .filter(|p| p.distance <= m.n("Range") && !listed(m.t("Ignore"), &p.name) && !(m.b("Ignore friends") && p.friend)).collect();
    let ids: HashSet<_> = players.iter().map(|p| p.id).collect();
    let new: Vec<_> = state(|s| {
        let new: Vec<_> = players.iter().filter(|p| !s.guarded.contains(&p.id)).map(|p| p.name.clone()).collect();
        s.guarded = ids;
        new
    });
    if !new.is_empty() {
        say(game, &format!("Base Guard: {} nearby.", new.join(", ")));
        if m.b("Log out") {
            let _ = game.leave_server(Some("Base Guard: player nearby.".into()));
            let _ = super::sunset_api::toggle(game, "Base Guard", false);
        }
    }
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    if module(mods, "Staff List").is_some() {
        let mut staff = state(|s| s.staff.iter().cloned().collect::<Vec<_>>());
        staff.sort();
        if !staff.is_empty() { out.text(&format!("Staff: {}", staff.join(", ")), [8.0, cam.h as f32 - 42.0], 0xFFFF_B020); }
    }
    let Some(m) = module(mods, "Chest Index").filter(|m| !m.t("Search").trim().is_empty()) else { return };
    let mut matches = state(|s| s.containers.iter().filter_map(|c| {
        let count: u32 = c.items.iter().filter(|(id, _)| listed(m.t("Search"), id) || short(id).contains(m.t("Search")))
            .map(|(_, count)| count).sum();
        (count > 0).then_some((c.pos, count))
    }).collect::<Vec<_>>());
    matches.sort_by(|a, b| dist(cam.pos, center_of(a.0)).total_cmp(&dist(cam.pos, center_of(b.0))));
    for (pos, count) in matches.into_iter().filter(|(p, _)| dist(cam.pos, center_of(*p)) <= m.n("Range")).take(m.n("Max results") as usize) {
        let lo = pos.map(f64::from);
        super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED, lo, lo.map(|v| v + 1.0), m.c("Color"), lw);
        if let Some([x, y]) = cam.project(center_of(pos)) {
            let label = format!("{count} {} ({}, {}, {})", m.t("Search"), pos[0], pos[1], pos[2]);
            out.text(&label, [x - game.native_text_width(&label) / 2.0, y], m.c("Color"));
        }
    }
}

pub(super) fn shutdown() {
    state(|s| *s = State::default());
}
