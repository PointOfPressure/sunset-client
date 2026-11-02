// Server-reported diagnostics. Passive mode never sends a diagnostic packet.
use super::Game;
use super::sunset::{Category, Module, say};
use super::sunset_server_scan_query::{self as query, QueryInfo};
use super::sunset_server_scan_signatures::{self as signatures, Kind, Strength};
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
const SUGGESTION_BASE: i32 = -0x5300_0000;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static EPOCH: AtomicU64 = AtomicU64::new(0);
static NEXT_SUGGESTION: AtomicU32 = AtomicU32::new(0);
static STATE: Mutex<Option<Session>> = Mutex::new(None);

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new("Server Scan", Category::Player, "Shows server-reported software, visible plugin commands and channels. Extended checks run only with .scan run.")
        .pick("Mode", &["Passive", "Extended"])
        .bool("Query", true)
        .pick("Query port", &["Auto", "Custom"])
        .num("Custom port", 25565.0, 1.0, 65535.0, 1.0)
        .bool("Command suggestions", true)
        .bool("Notifications", true)
        .bool("Show evidence", true)]
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Finding {
    name: String,
    kind: Kind,
    source: &'static str,
    evidence: String,
    strength: Strength,
    session: u64,
}

struct Pending {
    id: i32,
    expires: Instant,
    session: u64,
}
struct Session {
    id: u64,
    address: String,
    brand: String,
    remote_brand_fallback: bool,
    roots: Vec<String>,
    literals: Vec<String>,
    channels: Vec<String>,
    config_channels: Vec<String>,
    config_brand: Option<String>,
    suggestion_path: Option<String>,
    suggestions: Vec<String>,
    reported: Option<QueryInfo>,
    query_rx: Option<Receiver<(u64, Result<QueryInfo, String>)>>,
    query_error: Option<String>,
    pending: Option<Pending>,
    last_run: Option<Instant>,
    command_dirty: bool,
    dirty: bool,
    findings: Vec<Finding>,
    notified: Vec<String>,
    was_connected: bool,
}

impl Session {
    fn new(address: &str) -> Self {
        Self {
            id: EPOCH.fetch_add(1, Ordering::Relaxed) + 1,
            address: address.chars().take(256).collect(),
            brand: String::new(),
            remote_brand_fallback: true,
            roots: Vec::new(),
            literals: Vec::new(),
            channels: Vec::new(),
            config_channels: Vec::new(),
            config_brand: None,
            suggestion_path: None,
            suggestions: Vec::new(),
            reported: None,
            query_rx: None,
            query_error: None,
            pending: None,
            last_run: None,
            command_dirty: true,
            dirty: true,
            findings: Vec::new(),
            notified: Vec::new(),
            was_connected: false,
        }
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
    horizon_client::net::sunset_packets::scan_clear();
    horizon_client::net::sunset_packets::scan_enable(on);
    state(|s| *s = on.then(|| Session::new(address)));
}

pub(crate) fn brand(raw: &str) {
    if !active() {
        return;
    }
    state(|s| {
        if let Some(s) = s {
            s.brand = raw.chars().take(128).collect();
            s.dirty = true;
        }
    });
}

pub(crate) fn channel_id(id: &str) {
    if !active() || id.len() > 128 || !id.is_ascii() {
        return;
    }
    state(|s| {
        if let Some(s) = s {
            if s.channels.len() < MAX_CHANNELS && !s.channels.iter().any(|c| c == id) {
                s.channels.push(id.to_owned());
            }
        }
    });
}

fn config_channel(id: &str) {
    if !active() {
        return;
    }
    if id.len() > 128 || !id.is_ascii() {
        return;
    }
    state(|s| {
        if let Some(s) = s {
            if s.config_channels.len() < MAX_CHANNELS && !s.config_channels.iter().any(|c| c == id)
            {
                s.config_channels.push(id.to_owned());
            }
        }
    });
}

fn config_brand(raw: &str) {
    if active() {
        state(|s| {
            if let Some(s) = s {
                s.config_brand = Some(raw.chars().take(128).collect());
            }
        });
    }
}

pub(crate) fn commands_changed() {
    if active() {
        state(|s| {
            if let Some(s) = s {
                s.command_dirty = true;
            }
        });
    }
}

fn next_session(old: &Session, carry_config: bool) -> Session {
    let mut next = Session::new(&old.address);
    if carry_config {
        next.channels = old.config_channels.clone();
        next.brand = old.config_brand.clone().unwrap_or_default();
    }
    next.command_dirty = false;
    next.remote_brand_fallback = false;
    next.was_connected = true;
    next
}

pub(crate) fn reconfigured() {
    if active() {
        state(|slot| {
            if let Some(old) = slot.as_ref() {
                // Horizon can still hold the old command dispatcher here. Prefer missing evidence
                // over attributing the previous backend's products to the new backend.
                *slot = Some(next_session(old, true));
            }
        });
    }
}

fn invalidated() {
    state(|slot| {
        if let Some(old) = slot.as_ref() {
            *slot = Some(next_session(old, false));
        }
    });
}

pub(crate) fn disabled() {
    ACTIVE.store(false, Ordering::Relaxed);
    horizon_client::net::sunset_packets::scan_enable(false);
    state(|s| *s = None);
}

fn add(
    out: &mut Vec<Finding>,
    session: u64,
    name: &str,
    kind: Kind,
    source: &'static str,
    evidence: &str,
    strength: Strength,
) {
    if out.len() >= 128 || name.len() > 96 || evidence.len() > 256 {
        return;
    }
    if let Some(old) = out
        .iter_mut()
        .find(|f| f.name == name && f.source == source)
    {
        if matches!(
            (old.strength, strength),
            (Strength::Possible, Strength::Strong | Strength::Reported)
        ) {
            old.strength = strength;
            old.evidence = evidence.to_owned();
        }
    } else {
        out.push(Finding {
            name: name.to_owned(),
            kind,
            source,
            evidence: evidence.to_owned(),
            strength,
            session,
        });
    }
}

fn rebuild(s: &mut Session) {
    let mut findings = Vec::new();
    if let Some(hit) = signatures::brand(&s.brand) {
        add(
            &mut findings,
            s.id,
            hit.name,
            hit.kind,
            "Brand",
            &s.brand,
            hit.strength,
        );
    }
    for root in &s.roots {
        if let Some(hit) = signatures::command(root) {
            add(
                &mut findings,
                s.id,
                hit.name,
                hit.kind,
                "Command tree",
                root,
                hit.strength,
            );
        }
    }
    if let Some(info) = &s.reported {
        if let Some(software) = &info.software {
            if let Some(hit) = signatures::brand(software) {
                add(
                    &mut findings,
                    s.id,
                    hit.name,
                    hit.kind,
                    "Query",
                    software,
                    Strength::Reported,
                );
            }
        }
        for plugin in &info.plugins {
            let hit = signatures::query_plugin(plugin);
            let name = if hit.kind == Kind::Unknown {
                plugin
            } else {
                hit.name
            };
            add(
                &mut findings,
                s.id,
                name,
                hit.kind,
                "Query",
                plugin,
                hit.strength,
            );
        }
    }
    for text in &s.suggestions {
        let hit = signatures::query_plugin(text);
        if hit.kind != Kind::Unknown {
            add(
                &mut findings,
                s.id,
                hit.name,
                hit.kind,
                "Suggestion",
                text,
                Strength::Possible,
            );
        }
    }
    s.findings = findings;
}

pub(super) fn tick(game: &mut Game<'_>, module: Option<&Module>) {
    let Some(module) = module else {
        if active() {
            disabled();
        }
        state(|s| {
            if s.as_ref().is_some_and(|s| s.was_connected) {
                *s = None;
            }
        });
        return;
    };
    if game.remote.is_none() {
        ACTIVE.store(true, Ordering::Relaxed);
        horizon_client::net::sunset_packets::scan_enable(true);
        state(|s| {
            if s.as_ref().is_some_and(|s| s.was_connected) {
                *s = None;
            }
        });
        return;
    }
    let just_enabled = !ACTIVE.swap(true, Ordering::Relaxed);
    if just_enabled {
        horizon_client::net::sunset_packets::scan_enable(true);
    }
    for event in horizon_client::net::sunset_packets::scan_take() {
        use horizon_client::net::sunset_packets::ScanEvent;
        match event {
            ScanEvent::Brand(value) => brand(&value),
            ScanEvent::ConfigBrand(value) => config_brand(&value),
            ScanEvent::Channel(value) => channel_id(&value),
            ScanEvent::ConfigChannel(value) => config_channel(&value),
            ScanEvent::Commands => commands_changed(),
            ScanEvent::Reconfigured => reconfigured(),
            ScanEvent::Overflow => invalidated(),
        }
    }
    let inspect = state(|s| {
        if s.is_none() {
            *s = Some(Session::new("unknown"));
        }
        let session = s.as_mut().unwrap();
        session.was_connected = true;
        let due = just_enabled || session.command_dirty;
        session.command_dirty = false;
        due
    });
    let view = inspect
        .then(|| super::remote::sunset_server_scan_remote::view(game))
        .flatten();
    let mut notices = Vec::new();
    state(|s| {
        if let Some(s) = s {
            if let Some(view) = view {
                s.address = view.address;
                if s.brand.is_empty() && s.remote_brand_fallback {
                    s.brand = view.brand;
                }
                s.roots = view.roots.into_iter().take(MAX_ROOTS).collect();
                s.literals = view.literals;
                s.suggestion_path = view.suggestion;
                s.dirty = true;
            }
            if let Some(rx) = &s.query_rx {
                match rx.try_recv() {
                    Ok((id, result)) => {
                        if id == s.id {
                            match result {
                                Ok(info) => {
                                    s.reported = Some(info);
                                    s.query_error = None;
                                }
                                Err(e) => s.query_error = Some(e),
                            }
                            s.dirty = true;
                        }
                        s.query_rx = None;
                    }
                    Err(TryRecvError::Disconnected) => {
                        s.query_error = Some("Query worker stopped".into());
                        s.query_rx = None;
                        s.dirty = true;
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            if s.pending
                .as_ref()
                .is_some_and(|p| Instant::now() >= p.expires)
            {
                s.pending = None;
            }
            let changed = s.dirty;
            if changed {
                rebuild(s);
                s.dirty = false;
            }
            if changed && module.b("Notifications") {
                for f in &s.findings {
                    if f.kind != Kind::Unknown
                        && s.notified.len() < 16
                        && !s.notified.contains(&f.name)
                    {
                        s.notified.push(f.name.clone());
                        notices.push(format!("{} ({:?})", f.name, f.strength));
                    }
                }
            }
        }
    });
    if !notices.is_empty() {
        let displayed = notices
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let extra = if notices.len() > 4 {
            format!(" +{} more", notices.len() - 4)
        } else {
            String::new()
        };
        say(
            game,
            &format!("Server Scan: {displayed}{extra}. Use .scan details for evidence."),
        );
    }
}

fn owned_id(id: i32) -> bool {
    (SUGGESTION_BASE - 65535..=SUGGESTION_BASE).contains(&id)
}

fn take_pending(s: &mut Session, id: i32, now: Instant) -> Option<Pending> {
    if s.pending
        .as_ref()
        .is_some_and(|p| p.id == id && p.session == s.id && now < p.expires)
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
            let session = s.id;
            take_pending(s, id, Instant::now()).map(|p| (session, p))
        })
    });
    let Some((session, p)) = pending else {
        return true;
    };
    if p.id != id || p.session != session {
        return true;
    }
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
    if let Some(values) = values {
        state(|s| {
            if let Some(s) = s {
                if s.id == session {
                    s.suggestions = values;
                    s.dirty = true;
                }
            }
        });
    }
    true
}

pub(super) fn command(game: &mut Game<'_>, args: &[&str]) {
    let module = super::sunset::with(|s| s.module("Server Scan").cloned());
    let Some(module) = module.filter(|m| m.on) else {
        say(game, "Enable Server Scan first.");
        return;
    };
    if game.remote.is_none() {
        say(game, "Server Scan: connect to a server first.");
        return;
    }
    if args.first().copied() == Some("run") {
        if module.p("Mode") != "Extended" {
            say(game, "Select Extended mode before .scan run.");
            return;
        }
        let now = Instant::now();
        let request = state(|s| {
            let s = s.as_mut()?;
            if s.last_run
                .is_some_and(|last| now.duration_since(last) < Duration::from_secs(15))
                || s.query_rx.is_some()
            {
                return None;
            }
            let query = module.b("Query").then(|| (s.id, s.address.clone()));
            let suggestion_path = if module.b("Command suggestions") && s.pending.is_none() {
                s.suggestion_path.clone()
            } else {
                None
            };
            if query.is_none() && suggestion_path.is_none() {
                return Some((None, None));
            }
            s.last_run = Some(now);
            let suggestion = suggestion_path.map(|path| {
                let id = SUGGESTION_BASE
                    - (NEXT_SUGGESTION.fetch_add(1, Ordering::Relaxed) % 65536) as i32;
                s.pending = Some(Pending {
                    id,
                    expires: now + Duration::from_secs(5),
                    session: s.id,
                });
                (id, path)
            });
            Some((query, suggestion))
        });
        let Some((query_job, suggestion)) = request else {
            say(
                game,
                "Server Scan: wait for the current scan or 15-second cooldown.",
            );
            return;
        };
        if query_job.is_none() && suggestion.is_none() {
            say(
                game,
                "Server Scan: no enabled checks or visible suggestion path.",
            );
            return;
        }
        if let Some((id, address)) = query_job {
            if let Some((host, game_port)) = horizon_protocol::servers::parse_address(&address) {
                let port = if module.p("Query port") == "Custom" {
                    module.n("Custom port") as u16
                } else {
                    game_port
                };
                let (tx, rx) = channel();
                state(|s| {
                    if let Some(s) = s {
                        if s.id == id {
                            s.query_rx = Some(rx);
                            s.query_error = Some("waiting".into());
                        }
                    }
                });
                if let Err(error) = std::thread::Builder::new()
                    .name("sunset-server-scan-query".into())
                    .spawn(move || {
                        let _ = tx.send((id, query::query(&host, port)));
                    })
                {
                    state(|s| {
                        if let Some(s) = s {
                            if s.id == id {
                                s.query_rx = None;
                                s.query_error =
                                    Some(format!("could not start Query worker: {error}"));
                            }
                        }
                    });
                }
            } else {
                state(|s| {
                    if let Some(s) = s {
                        s.query_error = Some("invalid server address".into());
                    }
                });
            }
        }
        if let Some((id, path)) = suggestion {
            let mut writer = Writer::new();
            writer.varint(id);
            writer.string(&path);
            game.send(S::Unknown(
                horizon_protocol::ids::play::serverbound::COMMAND_SUGGESTION,
                writer.data,
            ));
        }
        say(game, "Server Scan: Extended checks started.");
        return;
    }
    let lines = state(|s| {
        let Some(s) = s else {
            return vec!["Server Scan: no server session yet.".to_owned()];
        };
        let mut lines = vec![format!(
            "Server Scan: {} | brand: {} | visible commands: {} | channels: {} | anti-cheat: {}",
            s.address,
            if s.brand.is_empty() {
                "Unknown"
            } else {
                &s.brand
            },
            s.roots.len(),
            s.channels.len(),
            if s.findings.iter().any(|f| f.kind == Kind::AntiCheat) {
                "see .scan plugins"
            } else {
                "Unknown"
            }
        )];
        match args.first().copied() {
            Some("plugins") => {
                for f in s.findings.iter().filter(|f| {
                    matches!(f.kind, Kind::Plugin | Kind::AntiCheat)
                        || (f.kind == Kind::Unknown && f.source == "Query")
                }) {
                    lines.push(format!("{} ({:?}, {:?})", f.name, f.kind, f.strength));
                }
            }
            Some("commands") => {
                lines.push(format!(
                    "Server-provided roots ({}), nested literals ({})",
                    s.roots.len(),
                    s.literals.len()
                ));
                for root in s.roots.iter().take(14) {
                    lines.push(format!("/{root}"));
                }
                for literal in s.literals.iter().take(12) {
                    lines.push(format!("/{literal}"));
                }
            }
            Some("details") => {
                for f in &s.findings {
                    lines.push(if module.b("Show evidence") {
                        format!(
                            "{} [{:?}/{:?}] {}: {}",
                            f.name, f.kind, f.strength, f.source, f.evidence
                        )
                    } else {
                        format!("{} [{:?}/{:?}]", f.name, f.kind, f.strength)
                    });
                }
                if module.b("Show evidence") {
                    for channel in s.channels.iter().take(16) {
                        lines.push(format!("Channel: {channel}"));
                    }
                }
                if let Some(info) = &s.reported {
                    if let Some(software) = &info.software {
                        lines.push(format!("Query software claim: {software}"));
                    }
                    lines.push(format!(
                        "Query Full Stat: version {}, map {}, players {} / {}",
                        info.version.as_deref().unwrap_or("Unknown"),
                        info.map.as_deref().unwrap_or("Unknown"),
                        info.players.map_or_else(|| "?".into(), |n| n.to_string()),
                        info.max_players
                            .map_or_else(|| "?".into(), |n| n.to_string())
                    ));
                    if let Some(hostname) = &info.hostname {
                        lines.push(format!("Query hostname: {hostname}"));
                    }
                    lines
                        .push("Query reports the UDP endpoint; a proxy backend may differ.".into());
                }
                if let Some(error) = &s.query_error {
                    lines.push(format!("Query: {error}"));
                }
            }
            _ => {
                for f in s.findings.iter().take(8) {
                    lines.push(format!("{} ({:?})", f.name, f.strength));
                }
            }
        }
        if lines.len() == 1 {
            lines.push("No known products detected; absence is not proof.".into());
        }
        lines.truncate(32);
        lines
    });
    for line in lines {
        say(game, &line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_aliases_are_one_source_and_unknown_ac_is_not_none() {
        let mut s = Session::new("example.org");
        s.roots = vec!["lp".into(), "luckperms:lp".into()];
        rebuild(&mut s);
        assert_eq!(
            s.findings.iter().filter(|f| f.name == "LuckPerms").count(),
            1
        );
        assert!(!s.findings.iter().any(|f| f.kind == Kind::AntiCheat));
    }

    #[test]
    fn vanilla_paper_and_reported_plugin_fixtures_keep_evidence_distinct() {
        let mut vanilla = Session::new("vanilla.example");
        vanilla.brand = "Vanilla".into();
        vanilla.roots = vec!["help".into(), "time".into()];
        rebuild(&mut vanilla);
        assert_eq!(vanilla.findings.len(), 1);
        assert_eq!(vanilla.findings[0].name, "Vanilla");

        let mut paper = Session::new("paper.example");
        paper.brand = "Paper".into();
        paper.roots = vec!["lp".into(), "luckperms:lp".into(), "co".into()];
        rebuild(&mut paper);
        assert_eq!(
            paper
                .findings
                .iter()
                .filter(|f| f.source == "Command tree")
                .count(),
            2
        );
        assert_eq!(
            paper
                .findings
                .iter()
                .find(|f| f.name == "LuckPerms")
                .unwrap()
                .strength,
            Strength::Strong
        );

        paper.reported = Some(QueryInfo {
            software: Some("Paper".into()),
            version: Some("1.21".into()),
            hostname: None,
            map: None,
            players: None,
            max_players: None,
            plugins: vec!["LuckPerms".into(), "GrimAC".into()],
        });
        rebuild(&mut paper);
        assert_eq!(
            paper
                .findings
                .iter()
                .filter(|f| f.name == "LuckPerms")
                .count(),
            2
        );
        assert!(
            paper
                .findings
                .iter()
                .any(|f| f.name == "GrimAC" && f.strength == Strength::Reported)
        );
    }

    #[test]
    fn suggestion_ids_do_not_own_anti_vanish_or_normal_ids() {
        assert!(owned_id(SUGGESTION_BASE - 1));
        assert!(!owned_id(-0x5355));
        assert!(!owned_id(42));
        let mut s = Session::new("example.org");
        let now = Instant::now();
        s.pending = Some(Pending {
            id: SUGGESTION_BASE - 2,
            expires: now + Duration::from_secs(5),
            session: s.id,
        });
        assert!(take_pending(&mut s, SUGGESTION_BASE - 1, now).is_none());
        assert!(
            s.pending.is_some(),
            "a stale reply must not steal the current request"
        );
        assert!(take_pending(&mut s, SUGGESTION_BASE - 2, now).is_some());
    }

    #[test]
    fn reconnect_changes_session_and_discards_old_findings() {
        let mut first = Session::new("one.example");
        first.brand = "Paper".into();
        rebuild(&mut first);
        let second = Session::new("two.example");
        assert_ne!(first.id, second.id);
        assert!(second.findings.is_empty());
    }

    #[test]
    fn reconfiguration_keeps_only_new_configuration_evidence() {
        let mut old = Session::new("proxy.example");
        old.brand = "Paper".into();
        old.roots.push("lp".into());
        old.channels.push("old:channel".into());
        old.config_channels.push("new:channel".into());
        old.config_brand = Some("Fabric".into());
        let next = next_session(&old, true);
        assert_ne!(next.id, old.id);
        assert_eq!(next.brand, "Fabric");
        assert_eq!(next.channels, ["new:channel"]);
        assert!(next.roots.is_empty());
        assert!(!next.remote_brand_fallback);
        let overflow = next_session(&old, false);
        assert!(overflow.brand.is_empty() && overflow.channels.is_empty());
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
            let mut s = Session::new("example.org");
            s.pending = Some(Pending {
                id: SUGGESTION_BASE,
                expires: Instant::now() + Duration::from_secs(5),
                session: s.id,
            });
            *slot = Some(s);
        });
        disabled();
        assert!(!active());
        state(|slot| assert!(slot.is_none()));
    }
}
