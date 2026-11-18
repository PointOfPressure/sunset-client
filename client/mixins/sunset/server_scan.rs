// Server-reported diagnostics. Passive mode never sends a diagnostic packet.
use super::Game;
use super::sunset::{Category, Module, say};
use super::sunset_server_scan_query::{self as query, QueryInfo};
use super::sunset_server_scan_signatures::{self as signatures, Kind, Match, Strength};
use horizon_client::net::sunset_packets::{self as packets, ScanEvent};
use horizon_protocol::{
    buf::{Reader, Writer},
    packets::play::Serverbound as S,
};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{Receiver, TryRecvError, channel},
    },
    time::{Duration, Instant},
};

const MAX_CHANNELS: usize = 128;
const MAX_ROOTS: usize = 512;
const MAX_FINDINGS: usize = 128;
const COOLDOWN: Duration = Duration::from_secs(15);
const SUGGESTION_BASE: i32 = -0x5300_0000;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static EPOCH: AtomicU64 = AtomicU64::new(0);
static NEXT_SUGGESTION: AtomicU32 = AtomicU32::new(0);
// One Query worker at a time, across reconnects too: a slow lookup must not pile threads up.
static QUERY_RUNNING: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<Session>> = Mutex::new(None);

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new("Server Scan", Category::Player, "Shows what the server reports about itself.")
        .pick("Mode", &["Passive", "Extended"])
        .bool("Query", true)
        .bool("Command suggestions", true)
        .bool("Notifications", true)
        .bool("Show evidence", true)]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Brand,
    Namespace,
    CommandTree,
    Channel,
    Query,
    Suggestions,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Source::Brand => "Brand",
            Source::Namespace => "Command namespace",
            Source::CommandTree => "Command tree",
            Source::Channel => "Channel",
            Source::Query => "Query",
            Source::Suggestions => "/version suggestions",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Finding {
    name: String,
    kind: Kind,
    strength: Strength,
    evidence: Vec<(Source, String)>,
}

// What one backend sent: the first configuration is 1, each proxy switch adds one.
#[derive(Default)]
struct Backend {
    generation: u32,
    config_seen: bool,
    config_channels: Vec<String>,
    play_channels: Vec<String>,
    brand: Option<String>,
}

impl Backend {
    fn new(generation: u32) -> Self {
        Self { generation, ..Self::default() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Check {
    NotRun,
    Skipped(&'static str),
    Waiting,
    Answered(usize),
    NoReply,
    Unreadable,
}

#[derive(Default)]
struct QueryState {
    rx: Option<Receiver<(u64, Result<QueryInfo, String>)>>,
    target: String,
    last_ok: Option<(QueryInfo, Instant)>,
    last_error: Option<(String, Instant)>,
}

impl QueryState {
    // The kept result is older than a failed attempt.
    fn stale(&self) -> bool {
        matches!((&self.last_ok, &self.last_error), (Some((_, ok)), Some((_, failed))) if failed > ok)
    }
}

struct Pending {
    id: i32,
    expires: Instant,
    session: u64,
    generation: u32,
}

struct Session {
    id: u64,
    address: String,
    // Turned on after joining: the first configuration went by unseen.
    late: bool,
    // The last configuration Server Scan did not see from its start.
    missed_config: u32,
    // The evidence queue overflowed and some events were dropped.
    incomplete: bool,
    switches: u32,
    cur: Backend,
    // Configuration evidence the network thread has for a backend the game has not switched to yet.
    next: Option<Backend>,
    // Horizon's stored brand, read when Server Scan missed the brand packet.
    fallback_brand: Option<String>,
    commands_seen: bool,
    roots: Vec<String>,
    literals: Vec<String>,
    suggestion_path: Option<String>,
    suggestions: Vec<String>,
    suggest: Check,
    query: QueryState,
    pending: Option<Pending>,
    last_run: Option<Instant>,
    command_dirty: bool,
    dirty: bool,
    findings: Vec<Finding>,
    notified: Vec<String>,
    // Results of .scan run, said at the next tick.
    notes: Vec<String>,
}

impl Session {
    // `missed_config`: the configuration that was already under way, or 0 on connecting.
    fn new(address: &str, generation: u32, missed_config: u32) -> Self {
        Self {
            id: EPOCH.fetch_add(1, Ordering::Relaxed) + 1,
            address: address.chars().take(256).collect(),
            late: missed_config > 0,
            missed_config,
            incomplete: false,
            switches: 0,
            cur: Backend::new(generation),
            next: None,
            fallback_brand: None,
            commands_seen: false,
            roots: Vec::new(),
            literals: Vec::new(),
            suggestion_path: None,
            suggestions: Vec::new(),
            suggest: Check::NotRun,
            query: QueryState::default(),
            pending: None,
            last_run: None,
            command_dirty: false,
            dirty: true,
            findings: Vec::new(),
            notified: Vec::new(),
            notes: Vec::new(),
        }
    }

    // Evidence the network thread tagged: the current backend's, or the next one's, ahead of the switch.
    fn backend(&mut self, generation: u32) -> Option<&mut Backend> {
        if generation == self.cur.generation {
            return Some(&mut self.cur);
        }
        if generation < self.cur.generation {
            return None;
        }
        if self.next.as_ref().is_none_or(|n| n.generation < generation) {
            self.next = Some(Backend::new(generation));
        }
        self.next.as_mut().filter(|n| n.generation == generation)
    }

    fn switch_to(&mut self, generation: u32) {
        if generation <= self.cur.generation {
            return;
        }
        let first = self.cur.generation == 0;
        self.cur = match self.next.take() {
            Some(next) if next.generation == generation => next,
            other => {
                self.next = other.filter(|n| n.generation > generation);
                Backend::new(generation)
            }
        };
        self.cur.config_seen = generation > self.missed_config;
        if !first {
            self.switches += 1;
        }
        // Horizon keeps the old backend's command tree until the new backend sends its own.
        self.roots.clear();
        self.literals.clear();
        self.suggestion_path = None;
        self.suggestions.clear();
        self.suggest = Check::NotRun;
        self.commands_seen = false;
        self.command_dirty = false;
        self.pending = None;
        self.fallback_brand = None;
        self.notified.clear();
        self.dirty = true;
    }

    fn apply(&mut self, event: ScanEvent) {
        match event {
            ScanEvent::ConfigChannel(g, id) => {
                if let Some(b) = self.backend(g) {
                    push_unique(&mut b.config_channels, id);
                }
            }
            ScanEvent::Brand(g, brand) => {
                if let Some(b) = self.backend(g) {
                    b.brand = Some(brand);
                }
            }
            // Play events come in the game's order: a newer number means the switch already happened.
            ScanEvent::Channel(g, id) => {
                self.switch_to(g);
                if g == self.cur.generation {
                    push_unique(&mut self.cur.play_channels, id);
                }
            }
            ScanEvent::Commands(g) => {
                self.switch_to(g);
                if g == self.cur.generation {
                    self.command_dirty = true;
                }
            }
            ScanEvent::Generation(g) => self.switch_to(g),
            // Lost events may include the next configuration's.
            ScanEvent::Overflow => {
                self.incomplete = true;
                self.command_dirty = true;
                self.missed_config = self.missed_config.max(self.cur.generation + 1);
                self.next = None;
            }
        }
        self.dirty = true;
    }
}

fn push_unique(list: &mut Vec<String>, id: String) {
    if list.len() < MAX_CHANNELS && !list.contains(&id) {
        list.push(id);
    }
}

fn state<R>(f: impl FnOnce(&mut Option<Session>) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

pub(crate) fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

pub(crate) fn connecting(address: &str) {
    let on = super::sunset::with(|s| s.module("Server Scan").is_some_and(|m| m.on));
    ACTIVE.store(on, Ordering::Relaxed);
    packets::scan_clear();
    packets::scan_enable(on);
    state(|s| *s = on.then(|| Session::new(address, 0, 0)));
}

pub(crate) fn disabled() {
    ACTIVE.store(false, Ordering::Relaxed);
    packets::scan_enable(false);
    state(|s| *s = None);
}

fn add(out: &mut Vec<Finding>, m: Match, name: &str, source: Source, evidence: &str) {
    if name.is_empty() || name.len() > 96 {
        return;
    }
    let evidence: String = evidence.chars().take(96).collect();
    if let Some(f) = out.iter_mut().find(|f| f.name.eq_ignore_ascii_case(name)) {
        f.strength = f.strength.min(m.strength);
        if f.kind == Kind::Unknown {
            f.kind = m.kind;
        }
        let same_source = f.evidence.iter().filter(|(s, _)| *s == source).count();
        if same_source < 3 && !f.evidence.iter().any(|(s, e)| *s == source && *e == evidence) {
            f.evidence.push((source, evidence));
        }
    } else if out.len() < MAX_FINDINGS {
        out.push(Finding { name: name.to_owned(), kind: m.kind, strength: m.strength, evidence: vec![(source, evidence)] });
    }
}

fn listed(out: &mut Vec<Finding>, raw: &str, source: Source, strength: Strength) {
    let m = signatures::query_plugin(raw);
    let name = if m.kind == Kind::Unknown { signatures::listed_name(raw) } else { m.name };
    add(out, Match { strength, ..m }, name, source, raw);
}

const BUKKIT_FAMILY: &[&str] = &["CraftBukkit", "Spigot", "Paper", "Folia", "Purpur", "Pufferfish"];

fn rebuild(s: &mut Session) {
    let mut out = Vec::new();
    let brand = s.cur.brand.as_deref().or(s.fallback_brand.as_deref()).unwrap_or("");
    let brands = signatures::brand(brand);
    for m in &brands {
        add(&mut out, *m, m.name, Source::Brand, brand);
    }
    for root in &s.roots {
        let source = if root.contains(':') { Source::Namespace } else { Source::CommandTree };
        if let Some(m) = signatures::command(root) {
            add(&mut out, m, m.name, source, &format!("/{root}"));
        } else if let Some(ns) = signatures::namespace(root) {
            // Bukkit names the namespace after the plugin: a plugin of that name is loaded.
            let unknown = Match { name: "", kind: Kind::Unknown, strength: Strength::Strong };
            add(&mut out, unknown, ns, Source::Namespace, &format!("/{root}"));
        }
    }
    // bukkit:, spigot: and paper: commands only restate a Bukkit-family brand; keep the most specific otherwise.
    if brands.iter().any(|m| BUKKIT_FAMILY.contains(&m.name)) {
        out.retain(|f| !f.name.ends_with("-based"));
    } else if out.iter().any(|f| f.name == "Paper-based") {
        out.retain(|f| f.name != "Spigot-based" && f.name != "Bukkit-based");
    } else if out.iter().any(|f| f.name == "Spigot-based") {
        out.retain(|f| f.name != "Bukkit-based");
    }
    for id in s.cur.config_channels.iter().chain(&s.cur.play_channels) {
        if let Some(m) = signatures::channel(id) {
            add(&mut out, m, m.name, Source::Channel, id);
        }
    }
    if let Some((info, _)) = &s.query.last_ok {
        if let Some(software) = &info.software {
            for m in signatures::brand(software) {
                add(&mut out, m, m.name, Source::Query, software);
            }
        }
        for plugin in &info.plugins {
            listed(&mut out, plugin, Source::Query, Strength::Reported);
        }
    }
    // Bukkit's /version completes the names of loaded plugins, unless another plugin replaced it.
    for text in &s.suggestions {
        listed(&mut out, text, Source::Suggestions, Strength::Strong);
    }
    let order = |k: Kind| match k {
        Kind::ServerSoftware => 0,
        Kind::Proxy => 1,
        Kind::Plugin | Kind::Unknown => 2,
        Kind::AntiCheat => 3,
    };
    out.sort_by(|a, b| {
        (order(a.kind), a.strength, a.name.to_ascii_lowercase()).cmp(&(order(b.kind), b.strength, b.name.to_ascii_lowercase()))
    });
    s.findings = out;
}

fn ago(d: Duration) -> String {
    match d.as_secs() {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h", s / 3600),
    }
}

fn query_summary(info: &QueryInfo) -> String {
    let mut parts = Vec::new();
    if let Some(software) = &info.software {
        parts.push(software.clone());
    }
    if let (Some(n), Some(max)) = (info.players, info.max_players) {
        parts.push(format!("{n}/{max} players"));
    }
    parts.push(match info.plugins.len() {
        0 => "no plugin list".to_owned(),
        1 => "1 plugin listed".to_owned(),
        n => format!("{n} plugins listed"),
    });
    parts.join(", ")
}

pub(super) fn tick(game: &mut Game<'_>, module: Option<&Module>) {
    let Some(module) = module else {
        if active() {
            disabled();
        }
        return;
    };
    if game.remote.is_none() {
        // Not connected: no network thread is running, so the configuration count starts over.
        if !ACTIVE.swap(true, Ordering::Relaxed) || state(|s| s.take().is_some()) {
            packets::scan_clear();
            packets::scan_enable(true);
        }
        return;
    }
    let just_enabled = !ACTIVE.swap(true, Ordering::Relaxed);
    if just_enabled {
        packets::scan_enable(true);
    }
    let events = packets::scan_take();
    let configuring = game.remote.as_ref().is_some_and(|r| packets::scan_configuring(&r.session));
    let inspect = state(|s| {
        // Turned on while connected: the configuration under way, or the last one, went by unseen.
        let s = s.get_or_insert_with(|| {
            let generation = packets::scan_generation();
            Session::new("unknown", generation, generation.max(1) + u32::from(configuring))
        });
        for event in events {
            s.apply(event);
        }
        let due = just_enabled || s.command_dirty || s.address == "unknown";
        s.command_dirty = false;
        due
    });
    let view = inspect.then(|| super::remote::sunset_server_scan_remote::view(game)).flatten();
    let now = Instant::now();
    let mut notices = Vec::new();
    let notes = state(|s| {
        let Some(s) = s else {
            return Vec::new();
        };
        if let Some(view) = view {
            s.address = view.address.chars().take(256).collect();
            if s.late && s.switches == 0 && s.cur.brand.is_none() && !view.brand.is_empty() {
                s.fallback_brand = Some(view.brand);
            }
            if let Some((roots, literals, suggestion)) = view.commands {
                s.roots = roots.into_iter().take(MAX_ROOTS).collect();
                s.literals = literals;
                s.suggestion_path = suggestion;
                s.commands_seen = true;
            }
            s.dirty = true;
        }
        if let Some(rx) = &s.query.rx {
            match rx.try_recv() {
                Ok((id, result)) => {
                    s.query.rx = None;
                    if id == s.id {
                        match result {
                            Ok(info) => {
                                s.notes.push(format!("Server Scan: Query answered: {}.", query_summary(&info)));
                                s.query.last_ok = Some((info, now));
                                s.query.last_error = None;
                            }
                            Err(error) => {
                                let kept = s.query.last_ok.as_ref().map_or(String::new(), |(_, at)| {
                                    format!(" Keeping the result from {} ago, marked as cached.", ago(now - *at))
                                });
                                s.notes.push(format!("Server Scan: Query failed: {error}.{kept}"));
                                s.query.last_error = Some((error, now));
                            }
                        }
                        s.dirty = true;
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    s.query.rx = None;
                    s.query.last_error = Some(("the Query worker stopped".into(), now));
                    s.notes.push("Server Scan: Query failed: the worker stopped.".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if s.pending.as_ref().is_some_and(|p| now >= p.expires) {
            s.pending = None;
            s.suggest = Check::NoReply;
            s.notes.push("Server Scan: the server did not answer the suggestion request.".into());
        }
        let changed = s.dirty;
        if changed {
            rebuild(s);
            s.dirty = false;
        }
        if changed && module.b("Notifications") {
            for f in &s.findings {
                if f.kind != Kind::Unknown && s.notified.len() < 16 && !s.notified.contains(&f.name) {
                    s.notified.push(f.name.clone());
                    notices.push(format!("{} ({})", f.name, f.strength.label()));
                }
            }
        }
        std::mem::take(&mut s.notes)
    });
    for note in notes {
        say(game, &note);
    }
    if !notices.is_empty() {
        let extra = if notices.len() > 4 { format!(" +{} more", notices.len() - 4) } else { String::new() };
        let shown = notices.into_iter().take(4).collect::<Vec<_>>().join(", ");
        say(game, &format!("Server Scan: {shown}{extra}. Use .scan for the summary."));
    }
}

fn owned_id(id: i32) -> bool {
    (SUGGESTION_BASE - 65535..=SUGGESTION_BASE).contains(&id)
}

fn take_pending(s: &mut Session, id: i32, now: Instant) -> Option<Pending> {
    if s.pending
        .as_ref()
        .is_some_and(|p| p.id == id && p.session == s.id && p.generation == s.cur.generation && now < p.expires)
    {
        s.pending.take()
    } else {
        None
    }
}

pub(crate) fn completions(data: &[u8]) -> bool {
    let mut reader = Reader::new(data);
    let Ok(id) = reader.varint() else {
        return false;
    };
    if !owned_id(id) {
        return false;
    }
    if data.len() > 16_384 {
        return true;
    }
    let pending = state(|s| {
        s.as_mut().and_then(|s| {
            let key = (s.id, s.cur.generation);
            take_pending(s, id, Instant::now()).map(|_| key)
        })
    });
    let Some(key) = pending else {
        return true;
    };
    let values = (|| {
        reader.varint().ok()?;
        reader.varint().ok()?;
        let n = reader.count(128).ok()?;
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            let text = reader.string(96).ok()?;
            if reader.bool().ok()? {
                reader.nbt().ok()?;
            }
            values.push(text);
        }
        Some(values)
    })();
    state(|s| {
        if let Some(s) = s
            && (s.id, s.cur.generation) == key
        {
            match values {
                Some(values) => {
                    s.notes.push(format!("Server Scan: /version suggested {} name(s).", values.len()));
                    s.suggest = Check::Answered(values.len());
                    s.suggestions = values;
                }
                None => {
                    s.notes.push("Server Scan: the suggestion answer could not be read.".into());
                    s.suggest = Check::Unreadable;
                }
            }
            s.dirty = true;
        }
    });
    true
}

const USAGE: &str = "Views: .scan plugins | details | channels | commands | status [page]";

pub(super) fn command(game: &mut Game<'_>, args: &[&str]) {
    let module = super::sunset::with(|s| s.module("Server Scan").cloned());
    let Some(module) = module.filter(|m| m.on) else {
        say(game, "Server Scan is off. Turn it on first (.toggle Server Scan).");
        return;
    };
    if game.remote.is_none() {
        say(game, "Server Scan: join a server first.");
        return;
    }
    let view = args.first().map(|a| a.to_ascii_lowercase());
    let page = |i: usize| args.get(i).and_then(|a| a.parse::<usize>().ok()).unwrap_or(1).max(1);
    if view.as_deref() == Some("run") {
        let lines = run(game, &module);
        for line in lines {
            say(game, &line);
        }
        return;
    }
    let now = Instant::now();
    let lines = state(|s| {
        let Some(s) = s else {
            return vec!["Server Scan: still waiting for the server; try again in a moment.".to_owned()];
        };
        match view.as_deref() {
            None | Some("summary") => overview(s, &module, now),
            Some("plugins") => plugins(s, page(1), now),
            Some("details") => details(s, &module, page(1), now),
            Some("channels") => channels(s, page(1)),
            Some("commands") => commands(s, &args[1..]),
            Some("status") => status(s, &module, now),
            Some("help") => vec![USAGE.to_owned(), ".scan run starts the Extended checks (Mode: Extended).".to_owned()],
            Some(other) => vec![format!("Server Scan: there is no \"{other}\" view."), USAGE.to_owned()],
        }
    });
    for line in lines {
        say(game, &line);
    }
}

// Splits entries into pages; a group's heading is repeated when it continues on a new page.
fn paginate(title: &str, entries: &[(&str, Vec<String>)], per_page: usize, page: usize, next: &str) -> Vec<String> {
    let pages = entries.len().div_ceil(per_page).max(1);
    let page = page.min(pages);
    let mut out = vec![if pages > 1 { format!("{title} ({page}/{pages})") } else { title.to_owned() }];
    let mut group = "";
    for &(g, ref lines) in entries.iter().skip((page - 1) * per_page).take(per_page) {
        if g != group && !g.is_empty() {
            out.push(format!("{g}:"));
        }
        group = g;
        out.extend(lines.iter().cloned());
    }
    if page < pages {
        out.push(format!("Next: {next} {}", page + 1));
    }
    out
}

fn names(s: &Session, kind: Kind) -> Vec<String> {
    s.findings.iter().filter(|f| f.kind == kind).map(|f| format!("{} ({})", f.name, f.strength.label())).collect()
}

fn not_observed(s: &Session) -> &'static str {
    if s.late && s.switches == 0 {
        "Server Scan was turned on after joining"
    } else if s.incomplete {
        "some evidence was dropped"
    } else {
        "it went by before Server Scan saw it"
    }
}

fn overview(s: &Session, module: &Module, now: Instant) -> Vec<String> {
    let mut out = vec![format!("Server Scan — {}", s.address)];
    let software = names(s, Kind::ServerSoftware);
    out.push(format!("Software: {}", if software.is_empty() { "Unknown".to_owned() } else { software.join(", ") }));
    let proxies = names(s, Kind::Proxy);
    if !proxies.is_empty() {
        out.push(format!("Proxy: {}", proxies.join(", ")));
    }
    let plugins = s.findings.iter().filter(|f| matches!(f.kind, Kind::Plugin | Kind::Unknown));
    let (possible, identified): (Vec<_>, Vec<_>) = plugins.partition(|f| f.strength == Strength::Possible);
    out.push(match (identified.len(), possible.len()) {
        (0, 0) => "Plugins: none identified".to_owned(),
        (i, p) => format!("Plugins: {i} identified, {p} possible"),
    });
    let anti = names(s, Kind::AntiCheat);
    out.push(if anti.is_empty() {
        "Anti-cheat: none identified (not proof that there is none)".to_owned()
    } else {
        format!("Anti-cheat: {}", anti.join(", "))
    });
    out.push(if s.commands_seen { format!("Commands: {} visible", s.roots.len()) } else { "Commands: not received yet".to_owned() });
    let observed = s.cur.config_channels.len() + s.cur.play_channels.len();
    out.push(format!("Channels: {observed} observed{}", if s.cur.config_seen { "" } else { " (Configuration not observed)" }));
    let extended = module.p("Mode") == "Extended";
    out.push(match (extended, s.last_run) {
        (false, _) => "Scan: Passive".to_owned(),
        (true, None) => "Scan: Extended (not run yet: .scan run)".to_owned(),
        (true, Some(at)) => format!("Scan: Extended (last run {} ago{})", ago(now - at), if s.query.stale() { "; Query result is cached" } else { "" }),
    });
    if s.switches > 0 {
        out.push(format!("The proxy switched backends {} time(s); this shows the current one.", s.switches));
    }
    out.push(format!("More: .scan plugins | details | channels | commands | status{}", if extended { " | run" } else { "" }));
    out
}

fn plugins(s: &Session, page: usize, now: Instant) -> Vec<String> {
    let group = |f: &Finding| match (f.kind, f.strength) {
        (Kind::AntiCheat, _) => (2, "Anti-cheat"),
        (_, Strength::Reported) => (0, "Reported by the server"),
        _ => (1, "Inferred from commands and channels"),
    };
    let mut found: Vec<&Finding> = s.findings.iter().filter(|f| matches!(f.kind, Kind::Plugin | Kind::Unknown | Kind::AntiCheat)).collect();
    found.sort_by_key(|f| group(f).0);
    let entries: Vec<(&str, Vec<String>)> = found
        .into_iter()
        .map(|f| {
            let unlisted = if f.kind == Kind::Unknown { " (not in Sunset's list)" } else { "" };
            (group(f).1, vec![format!("- {} — {}{unlisted}", f.name, f.strength.label())])
        })
        .collect();
    if entries.is_empty() {
        return vec![
            "Server Scan — Plugins".to_owned(),
            "No plugins or anti-cheats identified yet. Plugins can hide their commands, so this is not proof.".to_owned(),
        ];
    }
    let mut out = paginate("Server Scan — Plugins", &entries, 10, page, ".scan plugins");
    if let (true, Some((_, at))) = (s.query.stale(), &s.query.last_ok) {
        out.insert(1, format!("Query part is cached from {} ago; the latest Query failed.", ago(now - *at)));
    }
    if page >= entries.len().div_ceil(10) {
        out.push("Use .scan details for evidence.".to_owned());
    }
    out
}

fn query_label(q: &QueryState, now: Instant) -> String {
    match &q.last_ok {
        Some((_, at)) if q.stale() => format!("Query, cached from {} ago; the latest attempt failed", ago(now - *at)),
        Some((_, at)) => format!("Query, {} ago", ago(now - *at)),
        None => "Query".to_owned(),
    }
}

fn details(s: &Session, module: &Module, page: usize, now: Instant) -> Vec<String> {
    let evidence = module.b("Show evidence");
    let entries: Vec<(&str, Vec<String>)> = s
        .findings
        .iter()
        .map(|f| {
            let mut lines = vec![format!("{} — {}, {}", f.name, f.kind.label(), f.strength.label())];
            let mut sources: Vec<Source> = Vec::new();
            for (source, _) in &f.evidence {
                if !sources.contains(source) {
                    sources.push(*source);
                }
            }
            let label = |source: Source| if source == Source::Query { query_label(&s.query, now) } else { source.label().to_owned() };
            if evidence {
                for source in sources {
                    let items: Vec<&str> = f.evidence.iter().filter(|(s, _)| *s == source).map(|(_, e)| e.as_str()).collect();
                    lines.push(format!("  {}: {}", label(source), items.join(", ")));
                }
            } else {
                lines.push(format!("  Source: {}", sources.into_iter().map(label).collect::<Vec<_>>().join("; ")));
            }
            ("", lines)
        })
        .collect();
    if entries.is_empty() {
        return vec!["Server Scan — Details".to_owned(), "Nothing identified yet. Absence is not proof.".to_owned()];
    }
    paginate("Server Scan — Details", &entries, 4, page, ".scan details")
}

fn channels(s: &Session, page: usize) -> Vec<String> {
    let line = |id: &String| {
        let owner = signatures::channel(id).map_or(String::new(), |m| format!(" — {}", m.name));
        vec![format!("- {id}{owner}")]
    };
    let mut entries: Vec<(&str, Vec<String>)> = Vec::new();
    if !s.cur.config_seen {
        entries.push(("Configuration", vec![format!("- not observed: {}", not_observed(s))]));
    }
    entries.extend(s.cur.config_channels.iter().map(|id| ("Configuration", line(id))));
    if s.cur.config_seen && s.cur.config_channels.is_empty() {
        entries.push(("Configuration", vec!["- none sent".to_owned()]));
    }
    entries.extend(s.cur.play_channels.iter().map(|id| ("Play", line(id))));
    if s.cur.play_channels.is_empty() {
        entries.push(("Play", vec!["- none yet".to_owned()]));
    }
    let mut out = paginate("Server Scan — Channels", &entries, 10, page, ".scan channels");
    if page >= entries.len().div_ceil(10) {
        out.push("A channel name alone does not prove a plugin.".to_owned());
    }
    out
}

fn commands(s: &Session, args: &[&str]) -> Vec<String> {
    if !s.commands_seen {
        return vec!["Server Scan: the server has not sent its command list yet.".to_owned()];
    }
    let number = |a: Option<&&str>| a.and_then(|a| a.parse::<usize>().ok());
    // `.scan commands lp [page]`: one command's visible subcommands.
    if let Some(root) = args.first().copied().filter(|a| a.parse::<usize>().is_err()) {
        let root = root.trim_start_matches('/').to_ascii_lowercase();
        if !s.roots.iter().any(|r| r.eq_ignore_ascii_case(&root)) {
            return vec![format!("Server Scan: no visible command /{root}.")];
        }
        let prefix = format!("{root} ");
        let entries: Vec<(&str, Vec<String>)> = s
            .literals
            .iter()
            .filter(|l| l.to_ascii_lowercase().starts_with(&prefix))
            .map(|l| ("", vec![format!("/{l}")]))
            .collect();
        if entries.is_empty() {
            return vec![format!("/{root} has no visible subcommands.")];
        }
        return paginate(&format!("Server Scan — /{root}"), &entries, 10, number(args.get(1)).unwrap_or(1).max(1), &format!(".scan commands {root}"));
    }
    let mut plain: Vec<&str> = s.roots.iter().filter(|r| !r.contains(':')).map(String::as_str).collect();
    plain.sort_unstable();
    let mut namespaces: Vec<&str> = s.roots.iter().filter_map(|r| signatures::namespace(r)).collect();
    namespaces.sort_unstable();
    namespaces.dedup();
    // Several names to a line keeps a page short.
    let mut entries: Vec<(&str, Vec<String>)> = Vec::new();
    let mut line = String::new();
    for root in plain {
        if !line.is_empty() && line.len() + root.len() + 3 > 56 {
            entries.push(("", vec![std::mem::take(&mut line)]));
        }
        if !line.is_empty() {
            line.push_str(", ");
        }
        line.push('/');
        line.push_str(root);
    }
    if !line.is_empty() {
        entries.push(("", vec![line]));
    }
    let page = number(args.first()).unwrap_or(1).max(1);
    let title = format!("Server Scan — Commands ({} visible)", s.roots.len());
    let mut out = paginate(&title, &entries, 6, page, ".scan commands");
    if page >= entries.len().div_ceil(6) {
        let namespaced = s.roots.iter().filter(|r| r.contains(':')).count();
        if namespaced > 0 {
            let shown = namespaces.iter().take(8).copied().collect::<Vec<_>>().join(", ");
            let more = if namespaces.len() > 8 { ", …" } else { "" };
            out.push(format!("Plus {namespaced} namespaced (plugin: {shown}{more})."));
        }
        out.push("Subcommands: .scan commands <name>".to_owned());
    }
    out
}

fn status(s: &Session, module: &Module, now: Instant) -> Vec<String> {
    let extended = module.p("Mode") == "Extended";
    let mut out = vec!["Server Scan — Status".to_owned()];
    out.push(if extended { "Mode: Extended (checks run only with .scan run)".to_owned() } else { "Mode: Passive (sends nothing)".to_owned() });
    out.push(if s.late && s.switches == 0 {
        "Collection: watching since it was turned on (joined earlier)".to_owned()
    } else {
        "Collection: watching since joining".to_owned()
    });
    if s.switches > 0 {
        out.push(format!("Backend: switched {} time(s); evidence is for the current backend", s.switches));
    }
    out.push(if s.cur.config_seen {
        format!("Configuration: observed ({} channel(s))", s.cur.config_channels.len())
    } else {
        format!("Configuration: not observed ({})", not_observed(s))
    });
    out.push(if s.commands_seen { format!("Commands: {} root(s) received", s.roots.len()) } else { "Commands: not received yet".to_owned() });
    let q = &s.query;
    out.push(if !extended {
        "Query: not used in Passive mode".to_owned()
    } else if !module.b("Query") {
        "Query: off in settings".to_owned()
    } else if q.rx.is_some() {
        format!("Query: asking {}", q.target)
    } else {
        match (&q.last_ok, &q.last_error) {
            (_, Some((error, at))) if q.stale() || q.last_ok.is_none() => {
                let kept = q.last_ok.as_ref().map_or(String::new(), |(_, ok)| format!("; showing the result from {} ago", ago(now - *ok)));
                format!("Query: failed {} ago: {error}{kept}", ago(now - *at))
            }
            (Some((info, at)), _) => format!("Query: answered {} ago ({})", ago(now - *at), query_summary(info)),
            _ => "Query: not run".to_owned(),
        }
    });
    if let Some((info, _)) = &q.last_ok
        && let Some(version) = &info.version
    {
        out.push(format!("Query version: {version}{}", if s.switches > 0 { " (Query asks the address, not the backend)" } else { "" }));
    }
    out.push(if !extended {
        "Command suggestions: not used in Passive mode".to_owned()
    } else if !module.b("Command suggestions") {
        "Command suggestions: off in settings".to_owned()
    } else {
        match &s.suggest {
            Check::NotRun => "Command suggestions: not run".to_owned(),
            Check::Skipped(why) => format!("Command suggestions: skipped, {why}"),
            Check::Waiting => "Command suggestions: waiting for the answer".to_owned(),
            Check::Answered(n) => format!("Command suggestions: /version suggested {n} name(s)"),
            Check::NoReply => "Command suggestions: no answer".to_owned(),
            Check::Unreadable => "Command suggestions: the answer could not be read".to_owned(),
        }
    });
    if extended {
        let wait = s.last_run.map(|at| COOLDOWN.saturating_sub(now - at)).unwrap_or_default();
        out.push(if wait.is_zero() { "Next run: ready".to_owned() } else { format!("Next run: in {} s", wait.as_secs() + 1) });
    }
    if s.incomplete {
        out.push("Some events came too fast and were dropped; findings may be missing.".to_owned());
    }
    out
}

fn run(game: &mut Game<'_>, module: &Module) -> Vec<String> {
    if module.p("Mode") != "Extended" {
        return vec!["Server Scan: .scan run needs Mode set to Extended. Passive mode sends nothing.".to_owned()];
    }
    let now = Instant::now();
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    let plan = state(|s| {
        let s = s.as_mut()?;
        if let Some(at) = s.last_run {
            let wait = COOLDOWN.saturating_sub(now - at);
            if !wait.is_zero() {
                lines.push(format!("Server Scan: wait {} s before the next run.", wait.as_secs() + 1));
                return None;
            }
        }
        let query = if !module.b("Query") {
            skipped.push("Query: off in settings".to_owned());
            None
        } else if s.address == "local" {
            skipped.push("Query: single player has no Query".to_owned());
            None
        } else if s.query.rx.is_some() || QUERY_RUNNING.load(Ordering::Relaxed) {
            skipped.push("Query: the previous one is still running".to_owned());
            None
        } else if let Some((host, port)) = horizon_protocol::servers::parse_address(&s.address) {
            // Only the server you are on: its own host and port.
            Some((s.id, host, port))
        } else {
            skipped.push("Query: could not read the server address".to_owned());
            None
        };
        let suggestion = if !module.b("Command suggestions") {
            skipped.push("Command suggestions: off in settings".to_owned());
            None
        } else {
            let why = if !s.commands_seen {
                Some("the command list has not arrived yet")
            } else if s.pending.is_some() {
                Some("still waiting for the previous answer")
            } else if s.suggestion_path.is_none() {
                Some("no visible /version argument asks the server for suggestions")
            } else {
                None
            };
            if let Some(why) = why {
                if s.pending.is_none() {
                    s.suggest = Check::Skipped(why);
                }
                skipped.push(format!("Command suggestions: {why}"));
            }
            why.is_none().then(|| s.suggestion_path.clone()).flatten()
        };
        if query.is_none() && suggestion.is_none() {
            return None;
        }
        s.last_run = Some(now);
        let suggestion = suggestion.map(|path| {
            let id = SUGGESTION_BASE - (NEXT_SUGGESTION.fetch_add(1, Ordering::Relaxed) % 65536) as i32;
            s.pending = Some(Pending { id, expires: now + Duration::from_secs(5), session: s.id, generation: s.cur.generation });
            s.suggest = Check::Waiting;
            (id, path)
        });
        Some((query, suggestion))
    });
    let Some((query_job, suggestion)) = plan else {
        if lines.is_empty() {
            lines.push("Server Scan: no Extended check could start.".to_owned());
        }
        lines.extend(skipped.into_iter().map(|s| format!("Skipped — {s}")));
        return lines;
    };
    lines.push("Server Scan: Extended checks".to_owned());
    if let Some((id, host, port)) = query_job {
        let target = format!("{host}:{port}");
        match start_query(id, host, port) {
            Ok(rx) => {
                state(|s| {
                    if let Some(s) = s
                        && s.id == id
                    {
                        s.query.rx = Some(rx);
                        s.query.target = target.clone();
                    }
                });
                lines.push(format!("Started — Query to {target} over UDP"));
            }
            Err(error) => {
                state(|s| {
                    if let Some(s) = s
                        && s.id == id
                    {
                        s.query.last_error = Some((error.clone(), now));
                    }
                });
                lines.push(format!("Failed — Query: {error}"));
            }
        }
    }
    if let Some((id, path)) = suggestion {
        let mut writer = Writer::new();
        writer.varint(id);
        writer.string(&path);
        game.send(S::Unknown(horizon_protocol::ids::play::serverbound::COMMAND_SUGGESTION, writer.data));
        lines.push(format!("Sent — one suggestion request for {}", path.trim_end()));
    }
    lines.extend(skipped.into_iter().map(|s| format!("Skipped — {s}")));
    lines.push("Results follow in a few seconds (.scan status).".to_owned());
    lines
}

fn start_query(id: u64, host: String, port: u16) -> Result<Receiver<(u64, Result<QueryInfo, String>)>, String> {
    let generation = horizon_protocol::connection::sunset_lifecycle::RUNTIME.token();
    if QUERY_RUNNING.swap(true, Ordering::Relaxed) {
        return Err("the previous Query is still running".into());
    }
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("sunset-server-scan-query".into())
        .spawn(move || {
            if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.current(generation) {
                QUERY_RUNNING.store(false, Ordering::Relaxed);
                return;
            }
            let result = query::query(&host, port);
            QUERY_RUNNING.store(false, Ordering::Relaxed);
            if horizon_protocol::connection::sunset_lifecycle::RUNTIME.current(generation) { let _ = tx.send((id, result)); }
        })
        .map(|_| rx)
        .map_err(|error| {
            QUERY_RUNNING.store(false, Ordering::Relaxed);
            format!("could not start the Query worker ({error})")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(plugins: &[&str]) -> QueryInfo {
        QueryInfo {
            software: Some("Paper on 26.3".into()),
            version: Some("26.3".into()),
            hostname: None,
            map: None,
            players: None,
            max_players: None,
            plugins: plugins.iter().map(|p| (*p).into()).collect(),
        }
    }

    fn joined(address: &str) -> Session {
        let mut s = Session::new(address, 0, 0);
        s.apply(ScanEvent::Generation(1));
        s
    }

    #[test]
    fn command_aliases_are_one_finding_and_unknown_ac_is_not_none() {
        let mut s = joined("example.org");
        s.roots = vec!["lp".into(), "luckperms:lp".into()];
        rebuild(&mut s);
        let lp: Vec<_> = s.findings.iter().filter(|f| f.name == "LuckPerms").collect();
        assert_eq!(lp.len(), 1);
        assert_eq!(lp[0].strength, Strength::Strong);
        assert_eq!(lp[0].evidence.len(), 2);
        assert!(!s.findings.iter().any(|f| f.kind == Kind::AntiCheat));
    }

    #[test]
    fn vanilla_paper_and_reported_plugin_fixtures_keep_evidence_distinct() {
        let mut vanilla = joined("vanilla.example");
        vanilla.cur.brand = Some("Vanilla".into());
        vanilla.roots = vec!["help".into(), "time".into(), "minecraft:time".into()];
        rebuild(&mut vanilla);
        assert_eq!(vanilla.findings.len(), 1);
        assert_eq!(vanilla.findings[0].name, "Vanilla");

        let mut paper = joined("paper.example");
        paper.cur.brand = Some("Paper".into());
        paper.roots = vec!["lp".into(), "luckperms:lp".into(), "co".into(), "bukkit:version".into(), "mysteryplugin:open".into()];
        rebuild(&mut paper);
        assert!(!paper.findings.iter().any(|f| f.name.ends_with("-based")), "a Bukkit brand already says it");
        assert_eq!(paper.findings.iter().find(|f| f.name == "mysteryplugin").unwrap().kind, Kind::Unknown);
        paper.query.last_ok = Some((info(&["LuckPerms 5.4", "GrimAC 2.3", "MysteryPlugin 1.0"]), Instant::now()));
        rebuild(&mut paper);
        let lp = paper.findings.iter().find(|f| f.name == "LuckPerms").unwrap();
        assert_eq!(lp.strength, Strength::Reported);
        assert!(lp.evidence.iter().any(|(s, _)| *s == Source::Query));
        assert!(paper.findings.iter().any(|f| f.name == "GrimAC" && f.strength == Strength::Reported));
        assert_eq!(paper.findings.iter().filter(|f| f.name.eq_ignore_ascii_case("mysteryplugin")).count(), 1);
    }

    #[test]
    fn suggestion_ids_do_not_own_anti_vanish_or_normal_ids() {
        assert!(owned_id(SUGGESTION_BASE - 1));
        assert!(!owned_id(-0x5355));
        assert!(!owned_id(42));
        let mut s = joined("example.org");
        let now = Instant::now();
        s.pending = Some(Pending { id: SUGGESTION_BASE - 2, expires: now + Duration::from_secs(5), session: s.id, generation: 1 });
        assert!(take_pending(&mut s, SUGGESTION_BASE - 1, now).is_none());
        assert!(s.pending.is_some(), "a stale reply must not steal the current request");
        assert!(take_pending(&mut s, SUGGESTION_BASE - 2, now).is_some());
    }

    #[test]
    fn reconnect_changes_session_and_discards_old_findings() {
        let mut first = joined("one.example");
        first.cur.brand = Some("Paper".into());
        rebuild(&mut first);
        let second = Session::new("two.example", 0, 0);
        assert_ne!(first.id, second.id);
        assert!(second.findings.is_empty());
    }

    #[test]
    fn proxy_switch_keeps_each_backends_evidence_apart() {
        let mut s = Session::new("proxy.example", 0, 0);
        for event in [
            ScanEvent::ConfigChannel(1, "first:config".into()),
            ScanEvent::Brand(1, "Paper (Velocity)".into()),
            ScanEvent::Generation(1),
            ScanEvent::Commands(1),
        ] {
            s.apply(event);
        }
        assert!(s.command_dirty && s.cur.config_seen);
        s.command_dirty = false;
        s.commands_seen = true;
        s.roots = vec!["lp".into()];
        // The network thread starts the next backend while the game still has the old one's packets queued.
        for event in [
            ScanEvent::ConfigChannel(2, "second:config".into()),
            ScanEvent::Brand(2, "Purpur (Velocity)".into()),
            ScanEvent::Channel(1, "first:play".into()),
            ScanEvent::Commands(1),
        ] {
            s.apply(event);
        }
        assert_eq!(s.cur.config_channels, ["first:config"]);
        assert_eq!(s.cur.play_channels, ["first:play"]);
        assert_eq!(s.cur.brand.as_deref(), Some("Paper (Velocity)"));
        s.command_dirty = false;
        let now = Instant::now();
        s.pending = Some(Pending { id: SUGGESTION_BASE, expires: now + Duration::from_secs(5), session: s.id, generation: 1 });
        s.apply(ScanEvent::Generation(2));
        assert_eq!((s.cur.generation, s.switches), (2, 1));
        assert_eq!(s.cur.config_channels, ["second:config"]);
        assert!(s.cur.play_channels.is_empty());
        assert_eq!(s.cur.brand.as_deref(), Some("Purpur (Velocity)"));
        assert!(s.roots.is_empty() && !s.commands_seen && !s.command_dirty, "the old tree is not the new backend's");
        assert!(s.pending.is_none());
        // A late event from the old backend is dropped.
        s.apply(ScanEvent::Channel(1, "first:late".into()));
        s.apply(ScanEvent::ConfigChannel(1, "first:late".into()));
        assert!(s.cur.play_channels.is_empty() && s.cur.config_channels == ["second:config"]);
        rebuild(&mut s);
        assert!(s.findings.iter().any(|f| f.name == "Purpur"));
        assert!(!s.findings.iter().any(|f| f.name == "Paper" || f.name == "LuckPerms"));
    }

    #[test]
    fn respawns_keep_evidence_and_lost_switch_markers_still_switch() {
        let mut s = joined("example.org");
        s.apply(ScanEvent::Channel(1, "a:b".into()));
        s.apply(ScanEvent::Commands(1));
        assert_eq!(s.cur.play_channels, ["a:b"]);
        // Overflow lost Generation(2), but the next play event carries the new number.
        s.apply(ScanEvent::Overflow);
        s.apply(ScanEvent::Channel(2, "c:d".into()));
        assert_eq!((s.cur.generation, s.cur.play_channels.clone()), (2, vec!["c:d".to_owned()]));
        assert!(s.incomplete && !s.cur.config_seen);
    }

    #[test]
    fn turning_on_after_joining_is_not_a_complete_observation() {
        let mut s = Session::new("unknown", 3, 3);
        s.apply(ScanEvent::Channel(3, "a:b".into()));
        assert!(!s.cur.config_seen && s.switches == 0);
        assert_eq!(not_observed(&s), "Server Scan was turned on after joining");
        s.apply(ScanEvent::Generation(4));
        assert!(s.cur.config_seen && s.switches == 1);
    }

    #[test]
    fn failed_query_marks_the_kept_result_as_cached() {
        let mut s = joined("example.org");
        let then = Instant::now();
        s.query.last_ok = Some((info(&["GrimAC"]), then));
        assert!(!s.query.stale());
        let now = then + Duration::from_secs(120);
        assert_eq!(query_label(&s.query, now), "Query, 2 min ago");
        s.query.last_error = Some(("no reply".into(), now));
        assert!(s.query.stale());
        assert_eq!(query_label(&s.query, now), "Query, cached from 2 min ago; the latest attempt failed");
        rebuild(&mut s);
        assert!(s.findings.iter().any(|f| f.name == "GrimAC"), "kept for the session, labelled as cached");
    }

    #[test]
    fn pages_repeat_headings_and_point_to_the_next_page() {
        let entries: Vec<(&str, Vec<String>)> = (0..12).map(|i| (if i < 11 { "A" } else { "B" }, vec![format!("- {i}")])).collect();
        let first = paginate("T", &entries, 10, 1, ".scan x");
        assert_eq!(first[0], "T (1/2)");
        assert_eq!(first[1], "A:");
        assert_eq!(first.last().unwrap(), "Next: .scan x 2");
        let second = paginate("T", &entries, 10, 9, ".scan x");
        assert_eq!(second, ["T (2/2)", "A:", "- 10", "B:", "- 11"]);
    }

    #[test]
    fn passive_defaults_cannot_start_network_work() {
        let module = defaults().remove(0);
        assert_eq!(module.p("Mode"), "Passive");
        assert!(!active());
    }

    #[test]
    fn disabling_drops_session_and_pending_checks() {
        state(|slot| {
            let mut s = joined("example.org");
            s.pending = Some(Pending { id: SUGGESTION_BASE, expires: Instant::now() + Duration::from_secs(5), session: s.id, generation: 1 });
            *slot = Some(s);
        });
        disabled();
        assert!(!active());
        state(|slot| assert!(slot.is_none()));
    }
}
