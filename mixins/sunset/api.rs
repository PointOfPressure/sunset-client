// Sunset's plugin API, version 2 (version 1's calls work as they did).
//
// A plugin is another native mod bundle built together with Sunset. It adds a [[module]] of its own to
// crates/horizon-client/src/game.rs and hooks Horizon's native_tick to call `plugin(game, id, version, setup)` each
// tick; the first call runs `setup`, which adds modules, commands and event handlers. From then on Sunset runs them:
// at its own tick, for the packets it sends and sees, at its HUD and world drawing, on joins, disconnects, hurts,
// deaths, and when everything is stopped. Docs and two whole example plugins are in
// github.com/PointOfPressure/sunset-plugins.

use super::{Game, Screen};
use super::sunset::{Out, with};
use horizon_protocol::packets::play::{Clientbound, Serverbound};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[allow(dead_code)]
pub(super) const API_VERSION: u32 = 2;

// `const _: () = api::needs(2);` in a plugin: an older Sunset stops the build with this message.
#[allow(dead_code)]
pub(super) const fn needs(version: u32) {
    if version > API_VERSION {
        panic!("this plugin needs a newer Sunset plugin API than this Sunset has: update Sunset");
    }
}

#[allow(unused_imports)]
pub(super) use super::sunset::{Camera, Canvas, Category, Module, Shape, hold, line, look_at, outline, turn, turn_then};
#[allow(unused_imports)]
pub(super) use super::sunset_util::{
    PRIORITIES, Target, can_place, center_of, clear_line, dist, every_living, facts, find_hotbar, find_inventory, in_reach,
    is_air, listed, other_players, place_block, player_target, replaceable, sequence, short, show, toggle_off, turned_to,
};
// The game's packets, to read and send: `api::packets::Serverbound::Swing(..)`.
#[allow(unused_imports)]
pub(super) use horizon_protocol::packets::play as packets;

// Handler priorities: higher runs first; the default is NORMAL.
#[allow(dead_code)]
pub(super) const HIGHEST: i32 = 200;
#[allow(dead_code)]
pub(super) const HIGH: i32 = 100;
#[allow(dead_code)]
pub(super) const NORMAL: i32 = 0;
#[allow(dead_code)]
pub(super) const LOW: i32 = -100;
#[allow(dead_code)]
pub(super) const LOWEST: i32 = -200;

// ---------------------------------------------------------------- version 1

// Adds a module to the click GUI, the .modules list and the config file. Call it every tick if you like: a name that
// is already there is left alone.
#[allow(dead_code)]
pub(super) fn register(module: Module) {
    super::sunset::register_module(module);
}

// The module named `name` when the player has it on: its current settings.
#[allow(dead_code)]
pub(super) fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

// A line in chat from Sunset.
#[allow(dead_code)]
pub(super) fn chat(game: &mut Game<'_>, text: &str) {
    super::sunset::say(game, text);
}

// Turns a module off (its key bind and the GUI keep working).
#[allow(dead_code)]
pub(super) fn turn_off(game: &mut Game<'_>, name: &str) {
    toggle_off(game, name);
}

// ---------------------------------------------------------------- plugins and their handlers

type Run = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>) + Send>;
type Toggled = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &str, bool) + Send>;
type Sent = Box<dyn for<'a, 'b> FnMut(&'a Game<'b>, &mut Outgoing) + Send>;
type Seen = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &Clientbound) + Send>;
type Keyed = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &str, bool) + Send>;
type Painted = Box<dyn for<'a, 'b, 'c> FnMut(&'a Game<'b>, &mut Draw<'c>) + Send>;
type Left = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &Disconnect) + Send>;
type Hurt = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &Health) + Send>;
type Said = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &ChatLine) + Send>;
type Stopped = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &str) + Send>;
type Ran = Box<dyn for<'a, 'b> FnMut(&'a mut Game<'b>, &[&str]) + Send>;

enum On {
    PreTick(Run),
    Tick(Run),
    Toggle(Toggled),
    Out(Sent),
    In(Seen),
    Key(Keyed),
    Hud(Painted),
    World(Painted),
    Join(Run),
    Leave(Left),
    Hurt(Hurt),
    Death(Run),
    Chat(Said),
    Stop(Stopped),
}

const KINDS: usize = 14;

impl On {
    fn kind(&self) -> usize {
        match self {
            On::PreTick(_) => 0,
            On::Tick(_) => 1,
            On::Toggle(_) => 2,
            On::Out(_) => 3,
            On::In(_) => 4,
            On::Key(_) => 5,
            On::Hud(_) => 6,
            On::World(_) => 7,
            On::Join(_) => 8,
            On::Leave(_) => 9,
            On::Hurt(_) => 10,
            On::Death(_) => 11,
            On::Chat(_) => 12,
            On::Stop(_) => 13,
        }
    }
}

struct Listener {
    plugin: &'static str,
    priority: i32,
    on: On,
}

struct Loaded {
    id: &'static str,
    version: &'static str,
    modules: Vec<&'static str>,
    commands: Vec<(&'static str, &'static str)>,
    // Why it was switched off (a handler panicked).
    failed: Option<String>,
}

struct Command {
    plugin: &'static str,
    name: &'static str,
    run: Ran,
}

static PLUGINS: Mutex<Vec<Loaded>> = Mutex::new(Vec::new());
static BUS: Mutex<[Vec<Listener>; KINDS]> = Mutex::new([const { Vec::new() }; KINDS]);
// A bit for each kind of event some plugin listens for: Sunset skips the rest without taking a lock.
static LIVE: AtomicU32 = AtomicU32::new(0);
static COMMANDS: Mutex<Vec<Command>> = Mutex::new(Vec::new());
// Lines for the chat that came up where there was no game to write them with.
static NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());

thread_local! {
    // The plugin whose code runs now (its setup, a handler or a task): its tasks and errors are put down to it.
    static CURRENT: Cell<Option<&'static str>> = const { Cell::new(None) };
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn note(text: String) {
    let mut notes = lock(&NOTES);
    if notes.len() < 50 {
        notes.push(text);
    }
}

fn flush_notes(game: &mut Game<'_>) {
    let notes = std::mem::take(&mut *lock(&NOTES));
    for n in notes {
        super::sunset::say(game, &n);
    }
}

fn panic_text(e: &(dyn std::any::Any + Send)) -> String {
    e.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| e.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic".to_owned())
}

fn failed(plugin: &'static str) -> bool {
    lock(&PLUGINS).iter().any(|p| p.id == plugin && p.failed.is_some())
}

// Runs a plugin's code as that plugin; a panic switches the plugin off (every handler, command and task of it) with a
// chat line that says where.
fn guarded<R>(plugin: &'static str, what: &str, f: impl FnOnce() -> R) -> Option<R> {
    let before = CURRENT.with(|c| c.replace(Some(plugin)));
    let result = catch_unwind(AssertUnwindSafe(f));
    CURRENT.with(|c| c.set(before));
    match result {
        Ok(r) => Some(r),
        Err(e) => {
            let why = format!("its {what} code panicked: {}", panic_text(&*e));
            if let Some(p) = lock(&PLUGINS).iter_mut().find(|p| p.id == plugin) {
                p.failed = Some(why.clone());
            }
            lock(&TASKS).retain(|t| t.plugin != Some(plugin));
            note(format!("Plugin {plugin} is off: {why}. Fix it and build again (.plugins shows this)."));
            None
        }
    }
}

// Every handler of a kind, highest priority first, each as its plugin. `call` says whether to stop there (a cancelled
// packet). The list is taken out while they run, so a handler may add handlers or call any of the API; a handler that
// sets off its own kind of event again (a packet handler sending a packet) is not called for that one.
fn each(kind: usize, mut call: impl FnMut(&mut On) -> bool) {
    if LIVE.load(Ordering::Relaxed) & (1 << kind) == 0 {
        return;
    }
    let mut list = std::mem::take(&mut lock(&BUS)[kind]);
    for l in &mut list {
        if failed(l.plugin) {
            continue;
        }
        if guarded(l.plugin, kind_name(kind), || call(&mut l.on)) == Some(true) {
            break;
        }
    }
    let mut bus = lock(&BUS);
    let added = std::mem::take(&mut bus[kind]);
    if !added.is_empty() {
        list.extend(added);
        list.sort_by_key(|l| std::cmp::Reverse(l.priority));
    }
    bus[kind] = list;
}

fn kind_name(kind: usize) -> &'static str {
    ["pre tick", "tick", "toggle", "packet out", "packet in", "key", "HUD", "world", "join", "disconnect", "hurt", "death", "chat", "stop"][kind]
}

fn listen(plugin: &'static str, priority: i32, on: On) {
    let kind = on.kind();
    let mut bus = lock(&BUS);
    let list = &mut bus[kind];
    let at = list.iter().position(|l| l.priority < priority).unwrap_or(list.len());
    list.insert(at, Listener { plugin, priority, on });
    LIVE.fetch_or(1 << kind, Ordering::Relaxed);
    if kind == 4 {
        horizon_client::net::sunset_packets::PLUGIN_PACKETS.store(true, Ordering::Relaxed);
    }
}

// Sunset's own commands: a plugin can't take these names.
const BUILT_IN: &[&str] = &[
    "auradebug", "scan", "goto", "help", "commands", "menu", "gui", "pages", "toggle", "t", "modules", "list", "settings",
    "get", "set", "movementpreset", "bind", "binds", "panic", "debug", "profile", "profiles", "config", "cfg", "preset",
    "presets", "reset", "friend", "friends", "f", "loadout", "loadouts", "waypoint", "waypoints", "wp", "vclip", "hclip",
    "plugins",
];

fn plain_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 32 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// The plugin `id` (version `version`): the first call runs `setup`, later ones do nothing, so call it from your
// native_tick hook every tick. `id` is your mod's id (lower case letters, digits, - and _).
#[allow(dead_code)]
pub(super) fn plugin(game: &mut Game<'_>, id: &'static str, version: &'static str, setup: impl FnOnce(&mut Plugin)) {
    if !game.native_window() {
        return;
    }
    {
        let mut plugins = lock(&PLUGINS);
        if plugins.iter().any(|p| p.id == id) {
            return;
        }
        plugins.push(Loaded { id, version, modules: Vec::new(), commands: Vec::new(), failed: None });
    }
    if !plain_name(id) || id == super::sunset::ID {
        let why = format!("its id \"{id}\" can't be used: use your mod's id, lower case letters, digits, - and _ (not \"sunset\")");
        if let Some(p) = lock(&PLUGINS).iter_mut().find(|p| p.id == id) {
            p.failed = Some(why.clone());
        }
        note(format!("Plugin {id} is off: {why}."));
        return;
    }
    let mut p = Plugin { id, priority: NORMAL };
    guarded(id, "setup", || setup(&mut p));
}

// What `setup` gets: add modules, commands and handlers. `.priority(n)` applies to the handlers added after it.
#[allow(dead_code)]
pub(super) struct Plugin {
    id: &'static str,
    priority: i32,
}

#[allow(dead_code)]
impl Plugin {
    pub(super) fn id(&self) -> &'static str {
        self.id
    }
    pub(super) fn priority(&mut self, priority: i32) -> &mut Self {
        self.priority = priority;
        self
    }
    fn on(&mut self, on: On) -> &mut Self {
        listen(self.id, self.priority, on);
        self
    }
    // A module of yours: the click GUI shows it and its settings, .bind, .set and the config work with it.
    pub(super) fn module(&mut self, module: Module) -> &mut Self {
        let name = module.name;
        if with(|s| s.find(name).is_some()) {
            note(format!("Plugin {}: a module called {name} is already there (Sunset's or another plugin's): give yours another name.", self.id));
            return self;
        }
        super::sunset::register_module(module);
        if let Some(p) = lock(&PLUGINS).iter_mut().find(|p| p.id == self.id) {
            p.modules.push(name);
        }
        self
    }
    // A chat command, `.name args...`; `about` is its line in .help (say what the arguments are).
    pub(super) fn command(
        &mut self,
        name: &'static str,
        about: &'static str,
        run: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &[&str]) + Send + 'static,
    ) -> &mut Self {
        let lower = name.to_ascii_lowercase();
        let taken = BUILT_IN.contains(&lower.as_str()) || lock(&COMMANDS).iter().any(|c| c.name.eq_ignore_ascii_case(name));
        if !plain_name(name) || taken {
            note(format!(
                "Plugin {}: the command .{name} can't be added: {}.",
                self.id,
                if taken { "that name is taken (see .help)" } else { "use letters, digits, - and _ only" }
            ));
            return self;
        }
        lock(&COMMANDS).push(Command { plugin: self.id, name, run: Box::new(run) });
        if let Some(p) = lock(&PLUGINS).iter_mut().find(|p| p.id == self.id) {
            p.commands.push((name, about));
        }
        self
    }
    // Each Sunset tick in a world, before the game moves the player and sends the tick's move packet: set keys and
    // the look here.
    pub(super) fn on_pre_tick(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>) + Send + 'static) -> &mut Self {
        self.on(On::PreTick(Box::new(f)))
    }
    // Each Sunset tick in a world, where Sunset's modules run (after the move packet).
    pub(super) fn on_tick(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>) + Send + 'static) -> &mut Self {
        self.on(On::Tick(Box::new(f)))
    }
    // Any module turned on or off (the GUI, a bind, .t, a profile, Panic, another plugin): its name and whether it is
    // on now. Seen at the next tick; a module already on when it first shows up counts as turned on.
    pub(super) fn on_toggle(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &str, bool) + Send + 'static) -> &mut Self {
        self.on(On::Toggle(Box::new(f)))
    }
    // The module `name` turned on.
    pub(super) fn on_enable(&mut self, name: &'static str, mut f: impl for<'a, 'b> FnMut(&'a mut Game<'b>) + Send + 'static) -> &mut Self {
        self.on_toggle(move |game, module, on| {
            if on && module == name {
                f(game);
            }
        })
    }
    // The module `name` turned off.
    pub(super) fn on_disable(&mut self, name: &'static str, mut f: impl for<'a, 'b> FnMut(&'a mut Game<'b>) + Send + 'static) -> &mut Self {
        self.on_toggle(move |game, module, on| {
            if !on && module == name {
                f(game);
            }
        })
    }
    // Every packet the client sends, before it goes: read it, `cancel()` it or `replace()` it.
    pub(super) fn on_packet_out(&mut self, f: impl for<'a, 'b> FnMut(&'a Game<'b>, &mut Outgoing) + Send + 'static) -> &mut Self {
        self.on(On::Out(Box::new(f)))
    }
    // Every packet the server sent, at the next tick after the game handled it (read only).
    pub(super) fn on_packet_in(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &Clientbound) + Send + 'static) -> &mut Self {
        self.on(On::In(Box::new(f)))
    }
    // A key or mouse button going down (true) or up, by its name: "key.keyboard.h", "key.mouse.left".
    pub(super) fn on_key(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &str, bool) + Send + 'static) -> &mut Self {
        self.on(On::Key(Box::new(f)))
    }
    // Each frame, over the HUD, in GUI pixels.
    pub(super) fn on_hud(&mut self, f: impl for<'a, 'b, 'c> FnMut(&'a Game<'b>, &mut Draw<'c>) + Send + 'static) -> &mut Self {
        self.on(On::Hud(Box::new(f)))
    }
    // Each frame in a world: boxes, lines and labels at places in the world (as ESP draws).
    pub(super) fn on_world(&mut self, f: impl for<'a, 'b, 'c> FnMut(&'a Game<'b>, &mut Draw<'c>) + Send + 'static) -> &mut Self {
        self.on(On::World(Box::new(f)))
    }
    // The first tick in a world after joining a server or opening a single player world (reconnects included).
    pub(super) fn on_join(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>) + Send + 'static) -> &mut Self {
        self.on(On::Join(Box::new(f)))
    }
    // The connection ended: a kick, the server going away, or you leaving.
    pub(super) fn on_disconnect(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &Disconnect) + Send + 'static) -> &mut Self {
        self.on(On::Leave(Box::new(f)))
    }
    // Your health went down since the last tick.
    pub(super) fn on_hurt(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &Health) + Send + 'static) -> &mut Self {
        self.on(On::Hurt(Box::new(f)))
    }
    // Your health went below `below` (once each time it does: it goes again after it came back up to `below`).
    pub(super) fn on_low_health(&mut self, below: f32, mut f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &Health) + Send + 'static) -> &mut Self {
        self.on_hurt(move |game, h| {
            if h.health < below && h.before >= below {
                f(game, h);
            }
        })
    }
    // You died (the death screen came up).
    pub(super) fn on_death(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>) + Send + 'static) -> &mut Self {
        self.on(On::Death(Box::new(f)))
    }
    // A line came into the chat from the server or a player (Sunset's own lines are left out).
    pub(super) fn on_chat(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &ChatLine) + Send + 'static) -> &mut Self {
        self.on(On::Chat(Box::new(f)))
    }
    // Everything was stopped (`stop_everything`, Panic): let go of whatever you hold or loop. The text says why.
    pub(super) fn on_stop(&mut self, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>, &str) + Send + 'static) -> &mut Self {
        self.on(On::Stop(Box::new(f)))
    }
}

// A packet on its way out.
#[allow(dead_code)]
pub(super) struct Outgoing {
    packet: Serverbound,
    cancelled: bool,
    replaced: bool,
}

fn is_move(p: &Serverbound) -> bool {
    matches!(p, Serverbound::MovePlayerPos(_) | Serverbound::MovePlayerPosRot(_) | Serverbound::MovePlayerRot(_) | Serverbound::MovePlayerStatus(_))
}

#[allow(dead_code)]
impl Outgoing {
    pub(super) fn packet(&self) -> &Serverbound {
        &self.packet
    }
    // The tick's move packet (position, look or on-ground): there is one a tick at most.
    pub(super) fn is_move(&self) -> bool {
        is_move(&self.packet)
    }
    // It doesn't go; handlers of lower priority don't see it.
    pub(super) fn cancel(&mut self) {
        self.cancelled = true;
    }
    pub(super) fn cancelled(&self) -> bool {
        self.cancelled
    }
    // Something else goes in its place. A move packet can only become another move packet and anything else can't
    // become one (one move packet a tick): such a change is refused with a chat line.
    pub(super) fn replace(&mut self, packet: Serverbound) {
        if is_move(&packet) != is_move(&self.packet) {
            let plugin = CURRENT.with(Cell::get).unwrap_or("?");
            note(format!("Plugin {plugin}: a packet can't be replaced by a move packet or a move packet by another kind (one move packet a tick); left as it was."));
            return;
        }
        self.packet = packet;
        self.replaced = true;
    }
}

// Why the connection ended.
#[allow(dead_code)]
pub(super) struct Disconnect {
    pub(crate) reason: String,
    // The server said so (a kick, a shutdown), rather than the connection failing.
    pub(crate) by_server: bool,
    // Horizon's Auto-reconnect will try again (it is on, and it was not a kick or ban).
    pub(crate) retry: bool,
}

#[allow(dead_code)]
pub(super) struct Health {
    pub(crate) health: f32,
    pub(crate) before: f32,
}

#[allow(dead_code)]
pub(super) struct ChatLine {
    pub(crate) text: String,
    // A player's message (else the server's).
    pub(crate) player: bool,
}

// ---------------------------------------------------------------- drawing

// Where a HUD or world handler draws. Colours are 0xAARRGGBB (text: 0xRRGGBB).
#[allow(dead_code)]
pub(super) struct Draw<'a> {
    out: &'a mut dyn Canvas,
    cam: Option<&'a Camera>,
    lw: f32,
    size: [f32; 2],
}

impl Canvas for Draw<'_> {
    fn fill(&mut self, r: [f32; 4], argb: u32) {
        self.out.fill(r, argb);
    }
    fn quad(&mut self, corners: [[f32; 2]; 4], argb: u32) {
        self.out.quad(corners, argb);
    }
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.out.text(text, at, rgb);
    }
}

#[allow(dead_code)]
impl Draw<'_> {
    // The screen's width and height in GUI pixels.
    pub(super) fn size(&self) -> [f32; 2] {
        self.size
    }
    // A filled rectangle: x, y, width, height.
    pub(super) fn rect(&mut self, r: [f32; 4], argb: u32) {
        self.out.fill(r, argb);
    }
    pub(super) fn label(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.out.text(text, at, rgb);
    }
    pub(super) fn line(&mut self, from: [f32; 2], to: [f32; 2], argb: u32) {
        let lw = self.lw;
        super::sunset::line(self, from, to, argb, lw);
    }
    // Where a place in the world is on the screen (None: behind you, or no world drawn: in HUD handlers).
    pub(super) fn project(&self, at: [f64; 3]) -> Option<[f32; 2]> {
        self.cam?.project(at)
    }
    // A box from one corner to the other, as ESP draws it (world handlers only); false when it is out of sight.
    pub(super) fn cuboid(&mut self, lo: [f64; 3], hi: [f64; 3], argb: u32) -> bool {
        let Some(cam) = self.cam else {
            return false;
        };
        let lw = self.lw;
        super::sunset_visual::draw_box(cam, self, &super::sunset_visual::Look::BOXED, lo, hi, argb, lw)
    }
    // The block at `pos` boxed.
    pub(super) fn block(&mut self, pos: [i32; 3], argb: u32) -> bool {
        let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
        self.cuboid(lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], argb)
    }
    // Text at a place in the world (world handlers only).
    pub(super) fn tag(&mut self, at: [f64; 3], text: &str, rgb: u32) -> bool {
        let Some(p) = self.project(at) else {
            return false;
        };
        self.out.text(text, p, rgb);
        true
    }
}

// How wide a line of text is in GUI pixels, in the HUD's font.
#[allow(dead_code)]
pub(super) fn text_width(game: &Game<'_>, text: &str) -> f32 {
    game.native_text_width(text)
}

// ---------------------------------------------------------------- the scheduler

struct Task {
    id: u64,
    plugin: Option<&'static str>,
    due: u64,
    every: Option<u64>,
    run: Run,
}

static TASKS: Mutex<Vec<Task>> = Mutex::new(Vec::new());
// Ids of tasks cancelled while they ran.
static CANCELLED: Mutex<Vec<u64>> = Mutex::new(Vec::new());
static NEXT_TASK: AtomicU64 = AtomicU64::new(1);
// Ticks in a world, as the scheduler counts them: it waits while you are not in one.
static CLOCK: AtomicU64 = AtomicU64::new(0);

fn schedule(ticks: u64, every: Option<u64>, run: Run) -> u64 {
    let id = NEXT_TASK.fetch_add(1, Ordering::Relaxed);
    let plugin = CURRENT.with(Cell::get);
    let due = CLOCK.load(Ordering::Relaxed) + ticks.max(1);
    lock(&TASKS).push(Task { id, plugin, due, every, run });
    id
}

// Runs `f` once, `ticks` ticks (20 a second) from now; returns an id for `cancel`. Ticks only count in a world, so a
// task waits out a reconnect.
#[allow(dead_code)]
pub(super) fn after(ticks: u64, f: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static) -> u64 {
    let mut f = Some(f);
    schedule(ticks, None, Box::new(move |game| {
        if let Some(f) = f.take() {
            f(game);
        }
    }))
}

// Runs `f` every `ticks` ticks, the first time `ticks` from now, until cancelled.
#[allow(dead_code)]
pub(super) fn every(ticks: u64, f: impl for<'a, 'b> FnMut(&'a mut Game<'b>) + Send + 'static) -> u64 {
    schedule(ticks, Some(ticks.max(1)), Box::new(f))
}

// Stops a task from `after` or `every`; false when there was none (it ran, or was cancelled already).
#[allow(dead_code)]
pub(super) fn cancel(id: u64) -> bool {
    let mut tasks = lock(&TASKS);
    let before = tasks.len();
    tasks.retain(|t| t.id != id);
    if tasks.len() != before {
        return true;
    }
    drop(tasks);
    lock(&CANCELLED).push(id);
    false
}

// How many tasks are waiting.
#[allow(dead_code)]
pub(super) fn tasks() -> usize {
    lock(&TASKS).len()
}

fn run_tasks(game: &mut Game<'_>) {
    let now = CLOCK.fetch_add(1, Ordering::Relaxed) + 1;
    let due: Vec<Task> = {
        let mut tasks = lock(&TASKS);
        let (due, wait) = std::mem::take(&mut *tasks).into_iter().partition(|t| t.due <= now);
        *tasks = wait;
        due
    };
    for mut t in due {
        if lock(&CANCELLED).contains(&t.id) {
            continue;
        }
        let ran = match t.plugin {
            Some(p) if failed(p) => continue,
            Some(p) => guarded(p, "scheduled task", || (t.run)(game)).is_some(),
            None => catch_unwind(AssertUnwindSafe(|| (t.run)(game))).map_err(|e| note(format!("A scheduled task panicked: {}", panic_text(&*e)))).is_ok(),
        };
        let cancelled = {
            let mut c = lock(&CANCELLED);
            let had = c.contains(&t.id);
            c.retain(|&i| i != t.id);
            had
        };
        if let (true, false, Some(every)) = (ran, cancelled, t.every) {
            t.due = now + every;
            lock(&TASKS).push(t);
        }
    }
    lock(&CANCELLED).clear();
}

// ---------------------------------------------------------------- any module's settings

// Every module (Sunset's and plugins'), with its settings, on or off.
#[allow(dead_code)]
pub(super) fn modules() -> Vec<Module> {
    super::sunset::snapshot().to_vec()
}

// A module by name (as .modules shows it; case and spaces don't matter), on or off.
#[allow(dead_code)]
pub(super) fn find_module(name: &str) -> Option<Module> {
    with(|s| s.find(name).map(|i| s.modules[i].clone()))
}

#[allow(dead_code)]
pub(super) fn is_on(name: &str) -> bool {
    with(|s| s.find(name).is_some_and(|i| s.modules[i].on))
}

fn no_module(name: &str) -> String {
    let n = super::sunset::norm(name);
    let near: Vec<&'static str> = with(|s| {
        s.modules
            .iter()
            .filter(|m| {
                let mn = super::sunset::norm(m.name);
                !n.is_empty() && (mn.contains(&n) || n.contains(&mn) || mn.starts_with(&n[..n.len().min(3)]))
            })
            .map(|m| m.name)
            .take(5)
            .collect()
    });
    if near.is_empty() {
        format!("No module called \"{name}\". .modules lists them all.")
    } else {
        format!("No module called \"{name}\". Did you mean {}?", near.join(", "))
    }
}

// Turns a module on or off (nothing when it already is). Its bind, the GUI and its own on/off work run as for a
// click in the GUI.
#[allow(dead_code)]
pub(super) fn toggle(game: &mut Game<'_>, name: &str, on: bool) -> Result<(), String> {
    let Some((i, now)) = with(|s| s.find(name).map(|i| (i, s.modules[i].on))) else {
        return Err(no_module(name));
    };
    if now != on {
        super::sunset::toggle(game, i, false);
    }
    Ok(())
}

// A value `set` takes: true/false, a number, or text (a choice's name, a list, a switch as "on"/"off").
#[allow(dead_code)]
pub(super) trait SettingValue {
    fn text(&self) -> String;
}

impl SettingValue for bool {
    fn text(&self) -> String {
        self.to_string()
    }
}

impl SettingValue for f64 {
    fn text(&self) -> String {
        self.to_string()
    }
}

impl SettingValue for f32 {
    fn text(&self) -> String {
        self.to_string()
    }
}

impl SettingValue for i32 {
    fn text(&self) -> String {
        self.to_string()
    }
}

impl SettingValue for u32 {
    fn text(&self) -> String {
        self.to_string()
    }
}

impl SettingValue for &str {
    fn text(&self) -> String {
        (*self).to_owned()
    }
}

impl SettingValue for String {
    fn text(&self) -> String {
        self.clone()
    }
}

// Changes a setting of any module, as `.set` does (numbers are kept within the setting's range). Ok: the setting as
// .settings shows it now ("Range: 4.5"); Err: what was wrong, and what there is.
#[allow(dead_code)]
pub(super) fn set(game: &Game<'_>, module: &str, setting: &str, value: impl SettingValue) -> Result<String, String> {
    use super::sunset::Value;
    let value = value.text();
    let Some(i) = with(|s| s.find(module)) else {
        return Err(no_module(module));
    };
    let result = with(|s| {
        let m = &mut s.modules[i];
        let want = super::sunset::norm(setting);
        let Some(st) = m.settings.iter_mut().find(|st| super::sunset::norm(st.name) == want) else {
            let names: Vec<&str> = m.settings.iter().map(|st| st.name).collect();
            return Err(format!("{} has no setting \"{setting}\". Its settings: {}.", m.name, names.join(", ")));
        };
        let hint = match &st.value {
            Value::Bool(_) => "true or false".to_owned(),
            Value::Num { min, max, .. } => format!("a number from {} to {}", super::sunset::num(*min), super::sunset::num(*max)),
            Value::Pick { of, .. } => format!("one of: {}", of.join(", ")),
            Value::Text(_) => "text".to_owned(),
        };
        if !st.parse(&value) {
            return Err(format!("\"{value}\" is not a value for {} > {}: give {hint}.", m.name, st.name));
        }
        Ok(st.shown())
    });
    if result.is_ok() {
        super::sunset::save(game);
    }
    result
}

// ---------------------------------------------------------------- reads

// You, as the game has you now.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(super) struct Me {
    pub(crate) pos: [f64; 3],
    pub(crate) eye: [f64; 3],
    pub(crate) velocity: [f64; 3],
    // Degrees: yaw 0 faces south, 90 west; pitch 90 looks straight down.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) health: f32,
    pub(crate) absorption: f32,
    pub(crate) food: i32,
    pub(crate) on_ground: bool,
    // "minecraft:overworld", "minecraft:the_nether", "minecraft:the_end".
    pub(crate) dimension: &'static str,
    // The hotbar slot in hand (0 to 8).
    pub(crate) slot: usize,
    pub(crate) dead: bool,
}

#[allow(dead_code)]
pub(super) fn me(game: &Game<'_>) -> Option<Me> {
    let p = game.player.as_ref()?;
    let n = game.native_player()?;
    let pos = [p.position.x, p.position.y, p.position.z];
    Some(Me {
        pos,
        eye: game.eye_position().unwrap_or(pos),
        velocity: [p.velocity.x, p.velocity.y, p.velocity.z],
        yaw: game.yaw,
        pitch: game.pitch,
        health: p.health.health(),
        absorption: p.health.absorption(),
        food: p.health.food.food,
        on_ground: p.ground.on_ground,
        dimension: n.dimension,
        slot: game.selected,
        dead: game.screen == Screen::Dead,
    })
}

// In a world, playing (no menu or death screen up), on a server or in single player.
#[allow(dead_code)]
pub(super) fn in_world(game: &Game<'_>) -> bool {
    super::sunset_util::in_world(game)
}

// The block at a place: "stone", "oak_log", "air" (no "minecraft:").
#[allow(dead_code)]
pub(super) fn block(game: &Game<'_>, pos: [i32; 3]) -> String {
    short(&facts(game, pos).name).to_owned()
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(super) struct Entity {
    pub(crate) id: u32,
    // "minecraft:zombie", "minecraft:player"...
    pub(crate) kind: String,
    // A player's name, a custom name, or what it is.
    pub(crate) name: String,
    pub(crate) pos: [f64; 3],
    pub(crate) health: f32,
    pub(crate) player: bool,
    pub(crate) friend: bool,
    // From you, in blocks.
    pub(crate) distance: f64,
}

// Every living thing the client knows of but you, nearest first.
#[allow(dead_code)]
pub(super) fn entities(game: &Game<'_>) -> Vec<Entity> {
    let here = me(game).map_or([0.0; 3], |m| m.pos);
    let own = super::sunset::own_id(game);
    let friends = with(|s| s.friends.clone());
    let mut out: Vec<Entity> = game
        .mobs
        .iter()
        .filter(|m| m.alive() && !m.removed && Some(m.id) != own)
        .map(|m| {
            let name = super::sunset_util::display_name(game, m);
            let player = m.kind.id == "minecraft:player";
            let pos = [m.position.x, m.position.y, m.position.z];
            Entity {
                id: m.id,
                kind: m.kind.id.to_string(),
                friend: player && friends.iter().any(|f| f.eq_ignore_ascii_case(&name)),
                name,
                pos,
                health: m.health,
                player,
                distance: dist(here, pos),
            }
        })
        .collect();
    out.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    out
}

// The other players, nearest first (friends too: see `friend`).
#[allow(dead_code)]
pub(super) fn players(game: &Game<'_>) -> Vec<Entity> {
    entities(game).into_iter().filter(|e| e.player).collect()
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Item {
    // "fishing_rod", "cod" (no "minecraft:").
    pub(crate) id: String,
    pub(crate) count: u32,
    pub(crate) damage: u32,
}

// What is in a slot: 0 to 8 the hotbar, 9 to 35 the rest of the inventory, 40 the off hand.
#[allow(dead_code)]
pub(super) fn item(game: &Game<'_>, slot: usize) -> Option<Item> {
    let stack = if slot == super::sunset_util::OFFHAND { game.offhand.as_ref() } else { game.inventory.get(slot)?.as_ref() };
    stack.map(|s| Item { id: short(&s.item).to_owned(), count: s.count, damage: s.damage })
}

// How many of an item you carry (inventory and off hand); `id` as in `Item`, or a comma list ("cod,salmon").
#[allow(dead_code)]
pub(super) fn count(game: &Game<'_>, id: &str) -> u32 {
    let held = (0..36).chain([super::sunset_util::OFFHAND]).filter_map(|i| item(game, i));
    held.filter(|it| listed(id, &it.id)).map(|it| it.count).sum()
}

// The first slot (hotbar first) holding an item: `id` as in `Item`, or a comma list.
#[allow(dead_code)]
pub(super) fn find_item(game: &Game<'_>, id: &str) -> Option<usize> {
    find_inventory(game, |name| listed(id, name))
}

// Who Kill Aura is hitting, while it is on.
#[allow(dead_code)]
pub(super) fn combat_target() -> Option<u32> {
    is_on("Kill Aura").then(super::sunset_aura::target_id).flatten()
}

// ---------------------------------------------------------------- friends

#[allow(dead_code)]
pub(super) fn friends() -> Vec<String> {
    with(|s| s.friends.clone())
}

#[allow(dead_code)]
pub(super) fn is_friend(name: &str) -> bool {
    with(|s| s.friends.iter().any(|f| f.eq_ignore_ascii_case(name)))
}

// Kill Aura, Trigger Bot, Aim Assist and the rest leave friends alone; saved with Sunset's config.
#[allow(dead_code)]
pub(super) fn add_friend(game: &Game<'_>, name: &str) {
    if !is_friend(name) {
        with(|s| s.flip_friend(name));
        super::sunset::save(game);
    }
}

#[allow(dead_code)]
pub(super) fn remove_friend(game: &Game<'_>, name: &str) {
    if is_friend(name) {
        with(|s| s.flip_friend(name));
        super::sunset::save(game);
    }
}

// ---------------------------------------------------------------- actions

// Sends a packet as the game does. Move packets and held slot changes are refused: the game sends one move packet a
// tick (a second one gets you kicked; turn the look with `turn`/`turn_then`), and `hold` changes the slot so the
// server sees it once. Block actions need the game's own sequence number: `sequence(game)`.
#[allow(dead_code)]
pub(super) fn send(game: &mut Game<'_>, packet: Serverbound) -> Result<(), String> {
    if is_move(&packet) {
        return Err("Move packets can't be sent by a plugin (one a tick; a second gets you kicked): use turn/turn_then for the look, or change the tick's own one in on_packet_out.".to_owned());
    }
    if matches!(packet, Serverbound::SetCarriedItem(_)) {
        return Err("Change the held slot with api::hold(game, slot): a second slot change flags anticheats.".to_owned());
    }
    if game.remote.is_none() {
        return Err("Not connected: send packets in a world only (check api::in_world).".to_owned());
    }
    game.send(packet);
    Ok(())
}

// A message (or "/command") to the server, as if typed in chat. At most 256 characters.
#[allow(dead_code)]
pub(super) fn say_server(game: &mut Game<'_>, text: &str) -> Result<(), String> {
    if text.chars().count() > 256 {
        return Err("A chat message takes 256 characters at most (servers kick longer ones).".to_owned());
    }
    if game.remote.is_none() {
        return Err("Not connected: chat in a world only.".to_owned());
    }
    game.send_chat(text);
    Ok(())
}

// A right click with what you hold, as the game's own use key does it (eat, cast a rod, throw, place...).
#[allow(dead_code)]
pub(super) fn use_item(game: &mut Game<'_>) -> Result<(), String> {
    if !in_world(game) {
        return Err("Use items in a world, with no menu open (check api::in_world).".to_owned());
    }
    game.use_item().map(|_| ()).map_err(|e| format!("The game couldn't use the item: {e}"))
}

// Hits an entity the way Kill Aura does (Criticals ride along when on; the hit goes at the start of the next tick, in
// vanilla's packet order).
#[allow(dead_code)]
pub(super) fn attack(game: &mut Game<'_>, id: u32) {
    super::sunset::attack(game, id, true, None);
}

static LEAVE: AtomicBool = AtomicBool::new(false);

// Leaves the server (or closes single player) on purpose, before the next frame: Auto-reconnect doesn't try again.
#[allow(dead_code)]
pub(super) fn disconnect(game: &mut Game<'_>) {
    if game.remote.is_some() {
        LEAVE.store(true, Ordering::Relaxed);
    }
}

// Horizon's own Auto-reconnect (Options > Extras), for this session: it retries a lost connection and a server
// shutting down or restarting (five tries, 5 to 60 seconds apart), never a kick or ban.
#[allow(dead_code)]
pub(super) fn auto_reconnect(game: &mut Game<'_>, on: bool) {
    game.settings.extras.auto_reconnect = on;
}

#[allow(dead_code)]
pub(super) fn auto_reconnect_on(game: &Game<'_>) -> bool {
    game.settings.extras.auto_reconnect
}

// The safe stop: every module off, held keys let go, every plugin's tasks dropped and its on_stop run. `why` goes in
// chat and to the on_stop handlers.
#[allow(dead_code)]
pub(super) fn stop_everything(game: &mut Game<'_>, why: &str) {
    super::sunset::all_off(game);
    game.keys.clear();
    game.left = false;
    game.right = false;
    stopped(game, why);
    super::sunset::say(game, &format!("Stopped everything: {why}"));
}

// ---------------------------------------------------------------- persistent data

static STORED: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());

fn data_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
}

// Keeps `value` under `key` for your plugin, across restarts and reconnects: a file in your mod's data folder
// (horizon-mods/.horizon-mod-data/<plugin>/<key>). A value that didn't change isn't written again.
#[allow(dead_code)]
pub(super) fn save(game: &Game<'_>, plugin: &str, key: &str, value: &str) -> Result<(), String> {
    if !data_name(plugin) || plugin == super::sunset::ID || !data_name(key) {
        return Err(format!("save(\"{plugin}\", \"{key}\"): use your plugin's id and a key of letters, digits, - _ and . (not \"sunset\")."));
    }
    let mut stored = lock(&STORED);
    match stored.iter_mut().find(|(p, k, _)| p == plugin && k == key) {
        Some((_, _, v)) if v == value => return Ok(()),
        Some((_, _, v)) => *v = value.to_owned(),
        None => stored.push((plugin.to_owned(), key.to_owned(), value.to_owned())),
    }
    drop(stored);
    game.native_set_setting(plugin, key, value);
    Ok(())
}

// What `save` kept under `key`, if anything.
#[allow(dead_code)]
pub(super) fn load(game: &Game<'_>, plugin: &str, key: &str) -> Option<String> {
    if !data_name(plugin) || plugin == super::sunset::ID || !data_name(key) {
        return None;
    }
    if let Some((_, _, v)) = lock(&STORED).iter().find(|(p, k, _)| p == plugin && k == key) {
        return Some(v.clone());
    }
    let v = game.native_setting(plugin, key)?;
    lock(&STORED).push((plugin.to_owned(), key.to_owned(), v.clone()));
    Some(v)
}

// ---------------------------------------------------------------- what Sunset calls

static LAST_ON: Mutex<Vec<(&'static str, bool)>> = Mutex::new(Vec::new());
static JOINED: AtomicBool = AtomicBool::new(false);
// Health at the last tick (f32 bits; NaN: none yet), and whether the death screen was up.
static LAST_HEALTH: AtomicU32 = AtomicU32::new(u32::MAX);
static DEAD: AtomicBool = AtomicBool::new(false);
static CHAT_SEEN: AtomicU64 = AtomicU64::new(u64::MAX);

// Before the game's own tick work (Sunset's pre_tick), in a world.
pub(super) fn pre_tick(game: &mut Game<'_>) {
    if game.player.is_none() {
        return;
    }
    each(0, |on| {
        if let On::PreTick(f) = on {
            f(game);
        }
        false
    });
}

// Sunset's tick (post_tick), in a world: packets seen, joins, toggles, health, chat, tasks, then the tick handlers.
pub(super) fn tick(game: &mut Game<'_>) {
    flush_notes(game);
    if game.player.is_none() {
        return;
    }
    if JOINED.swap(false, Ordering::Relaxed) {
        LAST_HEALTH.store(u32::MAX, Ordering::Relaxed);
        each(8, |on| {
            if let On::Join(f) = on {
                f(game);
            }
            false
        });
    }
    if LIVE.load(Ordering::Relaxed) & (1 << 4) != 0 {
        for packet in horizon_client::net::sunset_packets::plugin_packets() {
            each(4, |on| {
                if let On::In(f) = on {
                    f(game, &packet);
                }
                false
            });
        }
    }
    toggles(game);
    health(game);
    chat_lines(game);
    run_tasks(game);
    each(1, |on| {
        if let On::Tick(f) = on {
            f(game);
        }
        false
    });
    flush_notes(game);
}

fn toggles(game: &mut Game<'_>) {
    let mods = super::sunset::snapshot();
    let changed: Vec<(&'static str, bool)> = {
        let mut last = lock(&LAST_ON);
        let mut changed = Vec::new();
        for m in mods.iter() {
            match last.iter_mut().find(|(n, _)| *n == m.name) {
                Some((_, was)) if *was != m.on => {
                    *was = m.on;
                    changed.push((m.name, m.on));
                }
                Some(_) => {}
                None => {
                    last.push((m.name, m.on));
                    if m.on {
                        changed.push((m.name, true));
                    }
                }
            }
        }
        changed
    };
    for (name, now) in changed {
        each(2, |on| {
            if let On::Toggle(f) = on {
                f(game, name, now);
            }
            false
        });
    }
}

fn health(game: &mut Game<'_>) {
    let dead = game.screen == Screen::Dead;
    if dead && !DEAD.swap(true, Ordering::Relaxed) {
        each(11, |on| {
            if let On::Death(f) = on {
                f(game);
            }
            false
        });
    }
    if !dead {
        DEAD.store(false, Ordering::Relaxed);
    }
    let Some(now) = game.player.as_ref().map(|p| p.health.health()) else {
        return;
    };
    let before = f32::from_bits(LAST_HEALTH.swap(now.to_bits(), Ordering::Relaxed));
    if !before.is_nan() && now < before {
        let h = Health { health: now, before };
        each(10, |on| {
            if let On::Hurt(f) = on {
                f(game, &h);
            }
            false
        });
    }
}

fn chat_lines(game: &mut Game<'_>) {
    use horizon_client::chat::ChatSource;
    let (total, lines) = horizon_client::chat::sunset_chat_log::since(CHAT_SEEN.load(Ordering::Relaxed));
    CHAT_SEEN.store(total, Ordering::Relaxed);
    for l in lines {
        if matches!(l.source, ChatSource::Client) {
            continue;
        }
        let line = ChatLine { text: l.text, player: matches!(l.source, ChatSource::Player) };
        each(12, |on| {
            if let On::Chat(f) = on {
                f(game, &line);
            }
            false
        });
    }
}

// Once a frame, in a world or not: joins and disconnects as the connection saw them.
pub(super) fn frame(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    if LEAVE.swap(false, Ordering::Relaxed) && game.remote.is_some() {
        let _ = game.leave_server(None);
    }
    let events = horizon_client::net::sunset_packets::plugin_connection();
    if !game.native_on(super::sunset::ID) {
        return;
    }
    for event in events {
        match event {
            horizon_client::net::sunset_packets::Connection::Joined => JOINED.store(true, Ordering::Relaxed),
            horizon_client::net::sunset_packets::Connection::Left(reason, key, by_server) => {
                JOINED.store(false, Ordering::Relaxed);
                let retry = auto_reconnect_on(game) && horizon_client::reconnect::allowed(&reason, key.as_deref(), by_server);
                let d = Disconnect { reason, by_server, retry };
                each(9, |on| {
                    if let On::Leave(f) = on {
                        f(game, &d);
                    }
                    false
                });
            }
        }
    }
}

// Sunset's send hook, after its own modules changed the packet.
pub(super) fn packet_out(game: &Game<'_>, packet: &Serverbound) -> Out {
    if LIVE.load(Ordering::Relaxed) & (1 << 3) == 0 || !game.native_window() {
        return Out::Pass;
    }
    let mut out = Outgoing { packet: packet.clone(), cancelled: false, replaced: false };
    each(3, |on| {
        if let On::Out(f) = on {
            f(game, &mut out);
        }
        out.cancelled
    });
    match out {
        Outgoing { cancelled: true, .. } => Out::Drop,
        Outgoing { replaced: true, packet, .. } => Out::Replace(packet),
        _ => Out::Pass,
    }
}

pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    each(5, |on| {
        if let On::Key(f) = on {
            f(game, key, pressed);
        }
        false
    });
}

pub(super) fn hud(game: &Game<'_>, out: &mut dyn Canvas, size: [f32; 2], lw: f32) {
    let mut draw = Draw { out, cam: None, lw, size };
    each(6, |on| {
        if let On::Hud(f) = on {
            f(game, &mut draw);
        }
        false
    });
}

pub(super) fn world(game: &Game<'_>, cam: &Camera, out: &mut dyn Canvas, lw: f32) {
    let size = [cam.w as f32, cam.h as f32];
    let mut draw = Draw { out, cam: Some(cam), lw, size };
    each(7, |on| {
        if let On::World(f) = on {
            f(game, &mut draw);
        }
        false
    });
}

// Everything stopped (stop_everything, Panic): tasks dropped, then every plugin's on_stop.
pub(super) fn stopped(game: &mut Game<'_>, why: &str) {
    lock(&TASKS).clear();
    each(13, |on| {
        if let On::Stop(f) = on {
            f(game, why);
        }
        false
    });
}

// `.name args` for a plugin's command: whether there was one.
pub(super) fn command(game: &mut Game<'_>, name: &str, args: &[&str]) -> bool {
    let mut commands = std::mem::take(&mut *lock(&COMMANDS));
    let found = commands.iter_mut().find(|c| c.name.eq_ignore_ascii_case(name));
    let ran = match found {
        Some(c) if failed(c.plugin) => {
            super::sunset::say(game, &format!(".{name} is off: its plugin {} failed (see .plugins).", c.plugin));
            true
        }
        Some(c) => {
            guarded(c.plugin, "command", || (c.run)(game, args));
            true
        }
        None => false,
    };
    let mut kept = lock(&COMMANDS);
    commands.append(&mut kept);
    *kept = commands;
    drop(kept);
    flush_notes(game);
    ran
}

// The plugins' commands, for .help.
pub(super) fn help() -> Vec<(String, &'static str)> {
    lock(&PLUGINS).iter().flat_map(|p| p.commands.iter().map(|(n, about)| (format!(".{n}"), *about))).collect()
}

// .plugins
pub(super) fn list(game: &mut Game<'_>) {
    let lines: Vec<String> = lock(&PLUGINS)
        .iter()
        .map(|p| {
            let modules = if p.modules.is_empty() { "none".to_owned() } else { p.modules.join(", ") };
            let commands: Vec<String> = p.commands.iter().map(|(n, _)| format!(".{n}")).collect();
            let commands = if commands.is_empty() { "none".to_owned() } else { commands.join(" ") };
            let state = p.failed.as_ref().map_or(String::new(), |why| format!(" - OFF: {why}"));
            format!("{} {} - modules: {modules} - commands: {commands}{state}", p.id, p.version)
        })
        .collect();
    if lines.is_empty() {
        super::sunset::say(game, &format!("No plugins (plugin API {API_VERSION}). How to make one: github.com/PointOfPressure/sunset-plugins"));
        return;
    }
    super::sunset::say(game, &format!("Plugins ({}, plugin API {API_VERSION}):", lines.len()));
    for l in lines {
        super::sunset::say(game, &l);
    }
}
