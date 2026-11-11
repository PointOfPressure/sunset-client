// Debug Report (Client): `.debug [chat lines]`, the module's row or its bind copies a plain-text report to the
// clipboard (the game's own) and saves it in its configured folder as <date-time>.md: versions and server, the player, the
// input sent, the modules on with their changed settings, the anticheat alerts read from chat (counted per check),
// the server's position corrections and the disconnects, the last packets sent tick by tick, and the chat. Markdown
// in short labelled sections, for pasting into an AI or a bug report. It reads; it changes nothing, and it holds no
// secrets (no session token, no account UUID; chat commands and messages sent are named, not quoted).

use super::Game;
use super::sunset::{Category, Module};
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

pub(super) const NAME: &str = "Debug Report";
// tools/pack.py writes the commit it packs here; a build straight from the source folder says so.
const COMMIT: &str = "unpacked";
const HORIZON_PIN: &str = "66f3981";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Client, "Copies a debug report and saves it to Save folder.")
            .num("Chat lines", 100.0, 0.0, 500.0, 10.0)
            .num("Packet ticks", 40.0, 5.0, 200.0, 5.0)
            .num("Max size (k chars)", 20.0, 5.0, 100.0, 5.0)
            .text("Save folder", "debug"),
    ]
}

// ---------------------------------------------------------------- what the hooks note

struct Correction {
    at: SystemTime,
    tick: u64,
    from: Option<[f64; 3]>,
    to: [f64; 3],
}

static CORRECTIONS: Mutex<VecDeque<Correction>> = Mutex::new(VecDeque::new());
static CORRECTED: AtomicUsize = AtomicUsize::new(0);
// GUI Move's last change to a tick's PlayerInput: the wire tick, the game's flags and the ones sent.
static GUI_MOVE: Mutex<Option<(u64, u8, u8)>> = Mutex::new(None);

// A server position packet (a setback, or a teleport): where the player was and where it puts them.
pub(super) fn correction(game: &Game<'_>, p: &play::PlayerPosition) {
    use play::relative as rel;
    let from = game.player.as_ref().map(|pl| [pl.position.x, pl.position.y, pl.position.z]);
    let to = std::array::from_fn(|i| {
        let bit = [rel::X, rel::Y, rel::Z][i];
        if p.relatives & bit != 0 { from.map_or(0.0, |f| f[i]) + p.position[i] } else { p.position[i] }
    });
    CORRECTED.fetch_add(1, Ordering::Relaxed);
    let mut list = CORRECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    if list.len() == 30 {
        list.pop_front();
    }
    list.push_back(Correction { at: SystemTime::now(), tick: horizon_protocol::connection::sunset_wire::ticks(), from, to });
}

pub(super) fn gui_move_input(game_flags: u8, sent: u8) {
    *GUI_MOVE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((horizon_protocol::connection::sunset_wire::ticks(), game_flags, sent));
}

// ---------------------------------------------------------------- copying

pub(super) fn copy(game: &mut Game<'_>, chat_lines: Option<usize>) {
    use horizon_client::clipboard::Clipboard;
    let mods = super::sunset::snapshot();
    let m = mods.iter().find(|m| m.name == NAME);
    let setting = |name: &str, default: f64| m.map_or(default, |m| m.n(name));
    let limits = Limits {
        chat: chat_lines.unwrap_or(setting("Chat lines", 100.0) as usize).min(500),
        ticks: setting("Packet ticks", 40.0) as u64,
        size: (setting("Max size (k chars)", 20.0) * 1000.0) as usize,
    };
    let report = report(game, &limits);
    game.clipboard.set(&report);
    let folder = m.map_or("debug", |m| m.t("Save folder"));
    let folder = if folder.is_empty() { "debug" } else { folder };
    let path = std::path::Path::new(folder);
    let dir = if path.is_absolute() {
        path.to_path_buf()
    } else {
        game.native_data_dir(super::sunset::ID).join(path)
    };
    let saved = std::fs::create_dir_all(&dir).and_then(|()| {
        let base = chrono::Local::now().format("%Y-%m-%d_%H.%M.%S").to_string();
        let path = (1..)
            .map(|n| dir.join(if n == 1 { format!("{base}.md") } else { format!("{base}_{n}.md") }))
            .find(|p| !p.exists())
            .unwrap_or_else(|| dir.join(format!("{base}.md")));
        std::fs::write(&path, &report).map(|()| path)
    });
    let chars = report.chars().count();
    let line = match saved {
        Ok(path) => format!("Debug report copied to the clipboard ({chars} characters) and saved to {}", path.display()),
        Err(e) => format!("Debug report copied to the clipboard ({chars} characters); saving it failed: {e}"),
    };
    super::sunset::say(game, &line);
}

struct Limits {
    chat: usize,
    ticks: u64,
    size: usize,
}

fn report(game: &Game<'_>, limits: &Limits) -> String {
    let (ticks, sent) = horizon_protocol::connection::sunset_wire::recent();
    let chat = horizon_client::chat::sunset_chat_log::lines();
    let mut out = String::new();
    let _ = writeln!(out, "# Sunset debug report");
    let _ = writeln!(
        out,
        "Times are local ({}). \"Tick\" = client ticks ended (ClientTickEnd packets sent) since the game started; now {ticks}.",
        chrono::Local::now().format("UTC%:z")
    );
    header(&mut out, game);
    player(&mut out, game);
    input(&mut out, game);
    modules(&mut out);
    super::sunset_scaffold_normal::report(&mut out);
    super::sunset_scaffold_telly::report(&mut out);
    super::sunset_lag::report(&mut out);
    super::sunset_backtrack::report(&mut out);
    super::sunset_player::inventory_report(&mut out);
    super::sunset_flight::report(&mut out);
    wire_sequence(&mut out);
    anticheat(&mut out, &chat);
    // The packet log gets up to two fifths of the room, the chat what is left.
    let packets = packet_log(&sent, ticks, limits.ticks, limits.size * 2 / 5);
    out.push_str(&packets);
    let room = limits.size.saturating_sub(out.len());
    chat_log(&mut out, &chat, limits.chat, room);
    out
}

fn clock(at: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(at).format("%H:%M:%S").to_string()
}

fn v3(v: [f64; 3]) -> String {
    format!("{:.3} {:.3} {:.3}", v[0], v[1], v[2])
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

// ---------------------------------------------------------------- sections

fn header(out: &mut String, game: &Game<'_>) {
    let _ = writeln!(out, "\n## Versions and server");
    let _ = writeln!(out, "- Sunset: {} (commit {COMMIT}), made for Horizon {HORIZON_PIN}", super::sunset::VERSION);
    let _ = writeln!(
        out,
        "- Horizon: {} (commit {})",
        horizon_client::crash::version(),
        option_env!("HORIZON_COMMIT").unwrap_or("not stamped")
    );
    let _ = writeln!(out, "- OS: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(out, "- Time: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z"));
    let _ = writeln!(out, "- Protocol: Minecraft {} ({})", horizon_protocol::GAME_VERSION, horizon_protocol::PROTOCOL_VERSION);
    match &game.remote {
        Some(r) => {
            let place = if game.integrated.is_some() { " (single player, the integrated server)" } else { "" };
            let _ = writeln!(out, "- Server: {}{place}", r.session.address);
            let _ = writeln!(out, "- Brand: {}", if r.brand.is_empty() { "(none sent)" } else { r.brand.as_str() });
            let _ = writeln!(out, "- Player: {}", r.session.name);
        }
        None => {
            let _ = writeln!(out, "- Server: not connected");
        }
    }
    let ping = super::remote::sunset_combat_remote::latency(game);
    let _ = writeln!(out, "- Ping: {}", ping.map_or("unknown".to_owned(), |ms| format!("{ms} ms (player list)")));
    let tps = horizon_client::net::sunset_packets::tps();
    let _ = writeln!(
        out,
        "- TPS estimate: {}",
        tps.map_or("unknown (needs 3 s of the server's clock)".to_owned(), |(t, s)| format!("{t:.2} (the server's clock over the last {s:.0} s)"))
    );
    let _ = writeln!(out, "- Dimension: {}", game.dim.id());
    let _ = writeln!(out, "- Game mode: {:?}", game.mode);
}

fn item(stack: Option<&horizon_client::items::ItemStack>) -> String {
    stack.map_or("empty".to_owned(), |s| format!("{} x{}", s.item, s.count))
}

fn screen_name(game: &Game<'_>) -> &'static str {
    use super::Screen as Sc;
    match game.screen {
        Sc::Playing => "none (playing)",
        Sc::GameModeSwitcher => "game mode switcher",
        Sc::EmoteWheel => "emote wheel",
        Sc::HudEditor => "HUD widget editor",
        Sc::Paused => "pause menu",
        Sc::Options => "options",
        Sc::Chat => "chat",
        Sc::Creative => "creative inventory",
        Sc::Inventory => "inventory or container",
        Sc::Dead => "death screen",
        Sc::Stats => "statistics",
        Sc::Advancements => "advancements",
        Sc::WorldOptions => "world options",
        Sc::Friends => "friends",
        Sc::Social => "social interactions",
        Sc::CommandBlock => "command block",
        Sc::BlockEditor => "block editor",
        Sc::ServerLinks => "server links",
        Sc::ConfirmLink => "confirm link",
        Sc::DialogWarning => "dialog warning",
        Sc::Win => "end credits",
        Sc::DebugOptions => "debug options",
        Sc::Mod => "mod screen",
    }
}

fn player(out: &mut String, game: &Game<'_>) {
    let _ = writeln!(out, "\n## Player");
    let Some(p) = game.player.as_ref() else {
        let _ = writeln!(out, "- No player (not in a world).");
        return;
    };
    let pos = [p.position.x, p.position.y, p.position.z];
    let _ = writeln!(out, "- Position: {} (block {} {} {})", v3(pos), pos[0].floor(), pos[1].floor(), pos[2].floor());
    let _ = writeln!(out, "- Rotation: yaw {:.2} pitch {:.2}", game.yaw, game.pitch);
    let _ = writeln!(out, "- Velocity: {} blocks/tick", v3([p.velocity.x, p.velocity.y, p.velocity.z]));
    let _ = writeln!(
        out,
        "- On ground: {}; horizontal collision: {}; fall distance: {:.2}",
        yes(p.ground.on_ground),
        yes(p.collision.horizontal),
        p.fall_distance
    );
    let _ = writeln!(
        out,
        "- Sprinting: {}; sneak key: {}; crouching: {}; swimming: {}; elytra: {}; flying: {}",
        yes(p.sprinting),
        yes(p.shift_key_down),
        yes(p.local_crouching),
        yes(p.swimming),
        yes(p.fall_flying),
        yes(p.flying)
    );
    let _ = writeln!(
        out,
        "- Health: {:.1}; absorption: {:.1}; food: {} (saturation {:.1})",
        p.health.health(),
        p.health.absorption(),
        p.health.food.food,
        p.health.food.saturation
    );
    let effects: Vec<String> = game
        .potions
        .effects
        .list
        .iter()
        .map(|e| {
            let time = if e.duration < 0 { "infinite".to_owned() } else { format!("{}s", e.duration / 20) };
            format!("{} {} ({time})", e.id, e.amplifier + 1)
        })
        .collect();
    let _ = writeln!(out, "- Effects: {}", if effects.is_empty() { "none".to_owned() } else { effects.join(", ") });
    let _ = writeln!(out, "- Held (hotbar slot {}): {}", game.selected + 1, item(game.inventory.get(game.selected).and_then(Option::as_ref)));
    let _ = writeln!(out, "- Offhand: {}", item(game.offhand.as_ref()));
    let armour: Vec<String> = ["head", "chest", "legs", "feet"]
        .iter()
        .zip(&game.armor)
        .map(|(part, s)| format!("{part} {}", item(s.as_ref())))
        .collect();
    let _ = writeln!(out, "- Armour: {}", armour.join(", "));
    let gui = if super::sunset::gui_open() { " (Sunset's menu open)" } else { "" };
    let _ = writeln!(out, "- Screen: {}{gui}", screen_name(game));
}

fn input_flags(flags: u8) -> String {
    let names = ["forward", "back", "left", "right", "jump", "shift", "sprint"];
    let on: Vec<&str> = names.iter().enumerate().filter(|(i, _)| flags & (1 << i) != 0).map(|(_, n)| *n).collect();
    format!("0x{flags:02x} ({})", if on.is_empty() { "none".to_owned() } else { on.join(" ") })
}

fn move_flags(flags: u8) -> String {
    format!(" ground {} wall {}", flags & 1, (flags >> 1) & 1)
}

fn input(out: &mut String, game: &Game<'_>) {
    use horizon_protocol::connection::sunset_wire;
    let _ = writeln!(out, "\n## Input");
    let decoded = |s: Option<sunset_wire::Sent>| s.and_then(|s| S::decode(s.id, &s.payload).ok().map(|p| (s.tick, p)));
    let last_input = match decoded(sunset_wire::last_input()) {
        Some((t, S::PlayerInput(i))) => format!("{} at tick {t}", input_flags(i.flags)),
        _ => "none this game (the game sends one only when the keys change)".to_owned(),
    };
    let _ = writeln!(out, "- Last PlayerInput sent: {last_input}");
    let held = |k: winit::keyboard::KeyCode| game.keys.contains(&k) || game.physical_keys.contains(&k);
    use winit::keyboard::KeyCode as K;
    let keys: Vec<&str> = [(K::KeyW, "W"), (K::KeyA, "A"), (K::KeyS, "S"), (K::KeyD, "D"), (K::Space, "Space"), (K::ShiftLeft, "Shift"), (K::ControlLeft, "Ctrl")]
        .iter()
        .filter(|(k, _)| held(*k))
        .map(|(_, n)| *n)
        .collect();
    let _ = writeln!(out, "- Move keys held now: {}", if keys.is_empty() { "none".to_owned() } else { keys.join(" ") });
    let mods = super::sunset::snapshot();
    let on = |name: &str| mods.iter().any(|m| m.name == name && m.on);
    let gui_move = match *GUI_MOVE.lock().unwrap_or_else(|e| e.into_inner()) {
        Some((t, from, to)) => format!("last changed a PlayerInput at tick {t}: game's {} sent as {}", input_flags(from), input_flags(to)),
        None => "never changed a PlayerInput this session".to_owned(),
    };
    let _ = writeln!(out, "- GUI Move: {}; {gui_move}", if on("GUI Move") { "on" } else { "off" });
    let lost: Vec<String> = super::sunset_snap_tap::last().iter().map(|(l, w)| format!("{l:?} (lost to {w:?})")).collect();
    let _ = writeln!(
        out,
        "- Snap Tap: {}; last tick it took out: {}",
        if on("Snap Tap") { "on" } else { "off" },
        if lost.is_empty() { "nothing".to_owned() } else { lost.join(", ") }
    );
    let _ = writeln!(out, "- W Tap: {}; {}", if on("W Tap") { "on" } else { "off" }, super::sunset_melee::tap_report(game));
    let last_move = decoded(sunset_wire::last_move()).map(|(t, p)| format!("{} at tick {t}", packet_line(&p)));
    let _ = writeln!(out, "- Last move packet: {}", last_move.as_deref().unwrap_or("none this game"));
}

fn modules(out: &mut String) {
    let (profile, list) = super::sunset::debug_modules();
    let _ = writeln!(out, "\n## Modules ({} on; profile: {profile})", list.len());
    let _ = writeln!(out, "Settings listed only where they differ from the default.");
    for (name, key, changed) in list {
        let bind = key.map(|k| format!(" [bind {k}]")).unwrap_or_default();
        let settings: Vec<String> = changed.iter().map(|(s, v, d)| format!("{s} = {v} (default {d})")).collect();
        let rest = if settings.is_empty() { String::new() } else { format!(": {}", settings.join("; ")) };
        let _ = writeln!(out, "- {name}{bind}{rest}");
    }
}

// ---------------------------------------------------------------- anticheat alerts

struct Alert {
    ac: String,
    player: String,
    check: String,
    vl: Option<String>,
    detail: String,
}

// Which anticheat a prefix names ("Grim »", "[Intave]", "[NC+]", Horizon's "[Anticheat]"...).
fn ac_name(prefix: &str) -> Option<String> {
    let p = prefix.to_ascii_lowercase();
    let known = [
        ("grim", "Grim"),
        ("intave", "Intave"),
        ("spartan", "Spartan"),
        ("nc+", "NCP"),
        ("ncp", "NCP"),
        ("nocheat", "NCP"),
        ("vulcan", "Vulcan"),
        ("matrix", "Matrix"),
        ("anticheat", "Horizon"),
    ];
    if let Some((_, name)) = known.iter().find(|(k, _)| p.contains(k)) {
        return Some((*name).to_owned());
    }
    let word: String = prefix.chars().filter(|c| c.is_alphanumeric() || *c == '+' || *c == ' ').collect();
    let word = word.trim();
    (!word.is_empty()).then(|| word.to_owned())
}

// "<prefix> <player>": the prefix and the player (the last word).
fn split_player(head: &str) -> (&str, &str) {
    let head = head.trim_end();
    head.rsplit_once(' ').map_or(("", head), |(p, n)| (p.trim(), n))
}

fn number(s: &str) -> Option<String> {
    let s = s.trim();
    s.parse::<f64>().is_ok().then(|| s.to_owned())
}

// An alert line, in the formats of the anticheats on Sunset's test rig: Grim's "<prefix> <player> failed <Check>
// (x<vl>) <verbose>", Horizon's "[Anticheat] <player> failed <check> (VL <vl>): <detail>", NCP's "[NC+] <player>
// failed <Check>: <detail> (VL<n>)", Intave's "[Intave] <player>/ <message> <details> (+<added> -> <vl>)" and
// Spartan's "[Spartan] <player> >> <detection>[<level>] (<info>)". Another "<prefix> <player> failed <check> ..."
// counts only when its prefix names an anticheat.
fn alert(text: &str) -> Option<Alert> {
    let text = text.trim();
    if let Some((head, tail)) = text.split_once(" failed ") {
        let (prefix, player) = split_player(head);
        let found = |check: &str, vl: Option<String>, detail: &str, ac: String| {
            let check = check.trim();
            (!check.is_empty() && !check.contains(' ') && !player.is_empty()).then(|| Alert {
                ac,
                player: player.to_owned(),
                check: check.to_owned(),
                vl,
                detail: detail.trim().to_owned(),
            })
        };
        // Grim: (x<vl>).
        if let Some((check, rest)) = tail.split_once(" (x")
            && let Some((vl, detail)) = rest.split_once(')')
            && let Some(vl) = number(vl)
        {
            return found(check, Some(vl), detail, ac_name(prefix).unwrap_or_else(|| "Grim".to_owned()));
        }
        // Horizon: (VL <vl>): <detail>.
        if let Some((check, rest)) = tail.split_once(" (VL ")
            && let Some((vl, detail)) = rest.split_once(')')
            && let Some(vl) = number(vl)
        {
            return found(check, Some(vl), detail.trim_start_matches(':'), ac_name(prefix).unwrap_or_else(|| "Horizon".to_owned()));
        }
        let ac = ac_name(prefix)?;
        // NCP and the rest: <Check>: <detail>, a VL at the end or none.
        let (check, detail) = tail.split_once(':').unwrap_or_else(|| tail.split_once(' ').unwrap_or((tail, "")));
        let vl = detail
            .rsplit_once("VL")
            .and_then(|(_, v)| number(v.trim_matches(|c: char| !c.is_ascii_digit() && c != '.')));
        return found(check, vl, detail, ac);
    }
    // Intave: <player>/<trust> <message> (+<added> -> <vl>).
    if let Some((head, tail)) = text.rsplit_once(" (+")
        && let Some((_, vl)) = tail.split_once("->")
        && let Some(vl) = number(vl.trim_end_matches(')'))
        && let Some(slash) = head.find("/ ").or_else(|| head.find('/'))
    {
        let (prefix, player) = split_player(&head[..slash]);
        let message = head[slash + 1..].trim();
        let (check, detail) = message.split_once(" (").map_or((message, ""), |(c, d)| (c, d.trim_end_matches(')')));
        return (!player.is_empty() && !check.is_empty()).then(|| Alert {
            ac: ac_name(prefix).unwrap_or_else(|| "Intave".to_owned()),
            player: player.to_owned(),
            check: check.trim().to_owned(),
            vl: Some(vl),
            detail: detail.to_owned(),
        });
    }
    // Spartan: <player> >> <detection>[<level>] (<info>).
    if let Some((head, tail)) = text.split_once(" >> ")
        && let Some((check, rest)) = tail.split_once('[')
        && let Some((level, info)) = rest.split_once(']')
    {
        let (prefix, player) = split_player(head);
        let ac = ac_name(prefix)?;
        return (!player.is_empty() && !check.trim().is_empty()).then(|| Alert {
            ac,
            player: player.to_owned(),
            check: check.trim().to_owned(),
            vl: number(level),
            detail: info.trim().trim_start_matches('(').trim_end_matches(')').to_owned(),
        });
    }
    None
}

struct Count {
    ac: String,
    player: String,
    check: String,
    count: usize,
    vl: Option<String>,
    detail: String,
    first: SystemTime,
    last: SystemTime,
}

fn counts(chat: &[horizon_client::chat::sunset_chat_log::Line]) -> Vec<Count> {
    let mut out: Vec<Count> = Vec::new();
    for line in chat.iter().filter(|l| l.source == horizon_client::chat::ChatSource::Server) {
        let Some(a) = alert(&line.text) else {
            continue;
        };
        match out.iter_mut().find(|c| c.ac == a.ac && c.player == a.player && c.check == a.check) {
            Some(c) => {
                c.count += 1;
                c.last = line.at;
                if a.vl.is_some() {
                    c.vl = a.vl;
                }
                c.detail = a.detail;
            }
            None => out.push(Count {
                ac: a.ac,
                player: a.player,
                check: a.check,
                count: 1,
                vl: a.vl,
                detail: a.detail,
                first: line.at,
                last: line.at,
            }),
        }
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.check.cmp(&b.check)));
    out
}

fn cell(s: &str) -> String {
    let s: String = s.chars().take(80).collect();
    s.replace('|', "/")
}

fn anticheat(out: &mut String, chat: &[horizon_client::chat::sunset_chat_log::Line]) {
    let _ = writeln!(out, "\n## Anticheat flags");
    let found = counts(chat);
    let _ = writeln!(
        out,
        "Alert lines read from all {} chat lines kept (Grim, Intave, Spartan, NCP and Horizon formats; other \"<name> failed <check>\" lines under their prefix).",
        chat.len()
    );
    if found.is_empty() {
        let _ = writeln!(out, "- No alert lines in chat.");
    } else {
        let _ = writeln!(out, "| Anticheat | Player | Check | Count | Last VL | Last detail | First | Last |");
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|");
        for c in found.iter().take(40) {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                cell(&c.ac),
                cell(&c.player),
                cell(&c.check),
                c.count,
                c.vl.as_deref().unwrap_or("-"),
                cell(&c.detail),
                clock(c.first),
                clock(c.last)
            );
        }
        if found.len() > 40 {
            let _ = writeln!(out, "({} more checks left out)", found.len() - 40);
        }
    }
    let _ = writeln!(out, "\n### Server position packets (setbacks and teleports)");
    let list = CORRECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    if list.is_empty() {
        let _ = writeln!(out, "- None this game.");
    } else {
        let _ = writeln!(out, "{} this game; the newest {} here, newest first.", CORRECTED.load(Ordering::Relaxed), list.len().min(20));
        let _ = writeln!(out, "| Time | Tick | From | To | Moved |");
        let _ = writeln!(out, "|---|---|---|---|---|");
        for c in list.iter().rev().take(20) {
            let (from, moved) = match c.from {
                Some(f) => (v3(f), format!("{:.3}", (0..3).map(|i| (c.to[i] - f[i]).powi(2)).sum::<f64>().sqrt())),
                None => ("(no player yet)".to_owned(), "-".to_owned()),
            };
            let _ = writeln!(out, "| {} | {} | {from} | {} | {moved} |", clock(c.at), c.tick, v3(c.to));
        }
    }
    let _ = writeln!(out, "\n### Disconnects and kicks");
    let ends = horizon_client::net::sunset_packets::ends();
    if ends.is_empty() {
        let _ = writeln!(out, "- None this game.");
    }
    for (at, reason, from_server) in ends.iter().rev().take(10) {
        let by = if *from_server { "server's reason" } else { "connection" };
        let _ = writeln!(out, "- {} ({by}): {}", clock(*at), reason.replace('\n', " "));
    }
}

// The client ticks as they went on the wire: a tick with two moves is one the server rejects, whoever sent them.
fn wire_sequence(out: &mut String) {
    let s = horizon_protocol::connection::sunset_wire::sequence();
    let _ = writeln!(out, "\n### Tick sequencing (on the wire)");
    if s.extra_moves == 0 {
        let _ = writeln!(out, "- No client tick carried more than one move packet this game.");
        return;
    }
    let ticks: Vec<String> = s.last.iter().flatten().map(u64::to_string).collect();
    let _ = writeln!(out, "- {} client ticks carried more than one move packet; the latest at ticks {}.", s.extra_moves, ticks.join(", "));
}

// ---------------------------------------------------------------- packets and chat

fn action_name(action: i32) -> String {
    match action {
        0 => "start dig".into(),
        1 => "dig (new face)".into(),
        2 => "abort dig".into(),
        3 => "stop dig".into(),
        4 => "drop stack".into(),
        5 => "drop item".into(),
        6 => "release use".into(),
        7 => "swap hands".into(),
        8 => "stab".into(),
        n => format!("action {n}"),
    }
}

fn command_name(action: i32) -> String {
    match action {
        0 => "leave bed".into(),
        1 => "start sprint".into(),
        2 => "stop sprint".into(),
        3 => "start riding jump".into(),
        4 => "stop riding jump".into(),
        5 => "open vehicle inventory".into(),
        6 => "start elytra".into(),
        n => format!("action {n}"),
    }
}

fn packet_line(p: &S) -> String {
    match p {
        S::MovePlayerPos(m) => format!("MovePos {}{}", v3([m.x, m.y, m.z]), move_flags(m.flags)),
        S::MovePlayerPosRot(m) => format!("MovePosRot {} yaw {:.2} pitch {:.2}{}", v3([m.x, m.y, m.z]), m.yaw, m.pitch, move_flags(m.flags)),
        S::MovePlayerRot(m) => format!("MoveRot yaw {:.2} pitch {:.2}{}", m.yaw, m.pitch, move_flags(m.flags)),
        S::MovePlayerStatus(m) => format!("MoveStatus{}", move_flags(m.flags)),
        S::PlayerInput(i) => format!("PlayerInput {}", input_flags(i.flags)),
        S::PlayerCommand(c) => format!("PlayerCommand {}", command_name(c.action)),
        S::PlayerAction(a) => format!("PlayerAction {} at {:?} face {} seq {}", action_name(a.action), a.pos, a.direction, a.sequence),
        S::Attack(a) => format!("Attack entity {}", a.id),
        S::Interact(i) => format!("Interact entity {} hand {}{}", i.id, i.hand, if i.secondary { " sneaking" } else { "" }),
        S::UseItem(u) => format!("UseItem hand {} seq {} yaw {:.2} pitch {:.2}", u.hand, u.sequence, u.yaw, u.pitch),
        S::UseItemOn(u) => format!("UseItemOn {:?} face {} hand {} seq {}", u.pos, u.face, u.hand, u.sequence),
        S::SetCarriedItem(c) => format!("SetCarriedItem slot {}", c.slot),
        S::ContainerClick(c) => format!("ContainerClick container {} slot {} button {} mode {}", c.container, c.slot, c.button, c.input),
        S::ContainerClose(c) => format!("ContainerClose {}", c.container),
        S::AcceptTeleportation(t) => format!("AcceptTeleportation id {} at {}", t.id, v3([t.x, t.y, t.z])),
        S::PickItemFromBlock(b) => format!("PickItemFromBlock {:?}", b.pos),
        S::ClientTickEnd(_) => "ClientTickEnd".into(),
        // The rest by name only: chat, commands, payloads and keys stay out of the report.
        other => {
            let name = format!("{other:?}");
            name.split(['(', ' ', '{']).next().unwrap_or("?").to_owned()
        }
    }
}

// One line a packet, its tick first; a tick's packets all went before its ClientTickEnd, which is left out. The same
// packet again in a tick is counted on its line, and a run of ticks with nothing else sent is one line.
fn packet_log(sent: &[horizon_protocol::connection::sunset_wire::Sent], now: u64, ticks: u64, room: usize) -> String {
    let from = now.saturating_sub(ticks);
    let mut lines: Vec<(u64, String, usize)> = Vec::new();
    for s in sent.iter().filter(|s| s.tick >= from && s.id != horizon_protocol::ids::play::serverbound::CLIENT_TICK_END) {
        let text = match S::decode(s.id, &s.payload) {
            Ok(p) => packet_line(&p),
            Err(_) => format!("packet id {} ({} bytes, not read)", s.id, s.len),
        };
        match lines.last_mut() {
            Some((tick, last, n)) if *tick == s.tick && *last == text => *n += 1,
            _ => lines.push((s.tick, text, 1)),
        }
    }
    let mut shown: Vec<String> = Vec::new();
    let mut next = from;
    for (tick, text, n) in &lines {
        if *tick > next {
            shown.push(quiet(next, *tick - 1));
        }
        next = tick + 1;
        shown.push(if *n > 1 { format!("{tick} {text} (x{n})") } else { format!("{tick} {text}") });
    }
    if now > next {
        shown.push(quiet(next, now - 1));
    }
    // The newest that fit.
    let mut used = 0;
    let kept = shown.iter().rev().take_while(|l| {
        used += l.len() + 1;
        used <= room
    }).count();
    let mut out = String::new();
    let _ = writeln!(out, "\n## Packets sent (ticks {from} to {now}, oldest first)");
    let _ = writeln!(out, "Tick number first; each tick's packets went before its ClientTickEnd (left out).");
    if kept < shown.len() {
        let _ = writeln!(out, "({} older lines left out for size)", shown.len() - kept);
    }
    let _ = writeln!(out, "```");
    for l in &shown[shown.len() - kept..] {
        let _ = writeln!(out, "{l}");
    }
    let _ = writeln!(out, "```");
    out
}

fn quiet(from: u64, to: u64) -> String {
    if from == to { format!("{from} (nothing sent)") } else { format!("{from}-{to} (nothing sent)") }
}

fn chat_log(out: &mut String, chat: &[horizon_client::chat::sunset_chat_log::Line], want: usize, room: usize) {
    use horizon_client::chat::ChatSource;
    let lines: Vec<String> = chat
        .iter()
        .rev()
        .take(want)
        .map(|l| {
            let source = match l.source {
                ChatSource::Player => "player",
                ChatSource::Server => "server",
                ChatSource::Client => "client",
            };
            format!("[{}] [{source}] {}", clock(l.at), l.text)
        })
        .collect();
    let title = "\n## Chat (newest last, [time] [source])\n";
    let mut used = title.len() + 16;
    let kept = lines.iter().take_while(|l| {
        used += l.len() + 1;
        used <= room
    }).count();
    out.push_str(&title);
    if kept < lines.len() {
        let _ = writeln!(out, "(the {} oldest of the {} lines asked for left out for size)", lines.len() - kept, lines.len());
    }
    if lines.is_empty() {
        let _ = writeln!(out, "- No chat lines.");
    }
    let _ = writeln!(out, "```");
    for l in lines[..kept].iter().rev() {
        let _ = writeln!(out, "{l}");
    }
    let _ = writeln!(out, "```");
}

#[cfg(test)]
mod tests {
    use super::alert;

    #[test]
    fn alert_formats() {
        let grim = alert("Grim » pointofpressure failed Simulation (x102.0) .420000").unwrap();
        assert_eq!((grim.ac.as_str(), grim.player.as_str(), grim.check.as_str(), grim.vl.as_deref()), ("Grim", "pointofpressure", "Simulation", Some("102.0")));
        assert_eq!(grim.detail, ".420000");
        let horizon = alert("[Anticheat] Tester failed prediction (VL 456.7): moved up to 0.118 blocks off, sent back").unwrap();
        assert_eq!((horizon.ac.as_str(), horizon.check.as_str(), horizon.vl.as_deref()), ("Horizon", "prediction", Some("456.7")));
        let ncp = alert("[NC+] Tester failed Commands: issued too many commands too quickly (VL12)").unwrap();
        assert_eq!((ncp.ac.as_str(), ncp.check.as_str(), ncp.vl.as_deref()), ("NCP", "Commands", Some("12")));
        let intave = alert("[Intave] Tester/ moved incorrectly (Δ0.132919 / 🔍1) (+10.00 -> 10.00)").unwrap();
        assert_eq!((intave.ac.as_str(), intave.player.as_str(), intave.check.as_str(), intave.vl.as_deref()), ("Intave", "Tester", "moved incorrectly", Some("10.00")));
        let spartan = alert("[Spartan] Tester >> gravity-simulation[3] (y: .30)").unwrap();
        assert_eq!((spartan.ac.as_str(), spartan.check.as_str(), spartan.vl.as_deref()), ("Spartan", "gravity-simulation", Some("3")));
        assert!(alert("<r94_> sgudpi").is_none());
        assert!(alert("Steve failed to escape the Wither").is_none());
    }
}
