// Watchlist editing stays separate from ordinary module profiles. The editor
// is a view of watchlists-v1, never a second source of saved player lists.
use super::{Game, sunset as runtime, sunset_watch_config::{self as config, Config, Profile, Store}};
use std::sync::{Arc, Mutex};

const MODULE: &str = "Anti Vanish";

#[derive(Clone)]
struct Editor {
    id: String,
    addresses: String,
    players: String,
    command: String,
}

impl Editor {
    fn profile(profile: Option<&Profile>) -> Self {
        match profile {
            Some(p) => Self { id: p.id.clone(), addresses: p.addresses.join(", "), players: p.players.join(", "), command: p.command.clone() },
            None => Self { id: String::new(), addresses: String::new(), players: String::new(), command: "msg".into() },
        }
    }
}

#[derive(Clone)]
struct State {
    config: Arc<Config>,
    editor: Editor,
    selection: String,
    generation: u64,
    load_error: Option<String>,
    notices: Vec<String>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Option<State>) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}

fn snapshot() -> Option<State> {
    with(|s| s.clone())
}

fn module() -> Option<runtime::Module> {
    runtime::with(|s| s.module(MODULE).cloned())
}

fn selection(config: &Config, input: &str) -> Result<String, String> {
    let input = input.trim().to_ascii_lowercase();
    config.resolve(&input, None)?;
    Ok(input)
}

// Caller never holds either state lock while entering the other subsystem.
fn fields(editor: &Editor, selection: &str) {
    runtime::with(|s| {
        let Some(m) = s.modules.iter_mut().find(|m| m.name == MODULE) else { return; };
        for setting in &mut m.settings {
            match (setting.name, &mut setting.value) {
                ("Server profile", runtime::Value::Text(v)) => *v = selection.to_owned(),
                ("Edit profile", runtime::Value::Text(v)) => *v = editor.id.clone(),
                ("Server addresses", runtime::Value::Text(v)) => *v = editor.addresses.clone(),
                ("Watched players", runtime::Value::Text(v)) => *v = editor.players.clone(),
                ("Completion command", runtime::Value::Pick { i, of }) => *i = of.iter().position(|v| *v == editor.command).unwrap_or(0),
                _ => {}
            }
        }
    });
}

pub(super) fn ensure_loaded(game: &Game<'_>) {
    if with(|s| s.is_some()) { return; }
    let loaded = Store::new(game.native_data_dir(runtime::ID)).and_then(|store| store.load());
    let (config, load_error) = match loaded {
        Ok(config) => (config, None),
        Err(error) => (Config::defaults(), Some(error)),
    };
    let m = module();
    let selected = m.as_ref().map_or("auto", |m| m.t("Server profile"));
    let selected = selection(&config, selected).unwrap_or_else(|_| "auto".into());
    let edit_id = m.as_ref().map_or("donutsmp", |m| m.t("Edit profile"));
    let editor = Editor::profile(config.profile(edit_id).or_else(|| config.profiles.first()));
    let notices = load_error.as_ref().map(|e| vec![format!("Watchlists: {e}. Repair watchlists-v1 and restart the client to resume editing; the saved file was preserved.")]).unwrap_or_default();
    let state = State { config: Arc::new(config), editor, selection: selected, generation: 1, load_error, notices };
    // All disk work finished before publishing the authoritative configuration.
    with(|s| { if s.is_none() { *s = Some(state); } });
    reload_editor();
}

pub(super) fn reload_editor() {
    if let Some(s) = snapshot() { fields(&s.editor, &s.selection); }
}

fn notice(message: String) {
    with(|s| {
        if let Some(s) = s {
            // Bound pending output even while chat cannot be drawn.
            if s.notices.len() < 8 { s.notices.push(format!("Watchlists: {message}")); }
        }
    });
}

pub(super) fn take_notices() -> Vec<String> {
    with(|s| s.as_mut().map(|s| std::mem::take(&mut s.notices)).unwrap_or_default())
}

fn persist(game: &Game<'_>, before: &State, candidate: Config) -> Result<(), String> {
    if let Some(error) = &before.load_error { return Err(format!("cannot edit an unreadable watchlists-v1: {error}")); }
    candidate.validate()?;
    Store::new(game.native_data_dir(runtime::ID))?.save(&candidate)?;
    // Only a completed checked write replaces the in-memory source of truth.
    with(|s| {
        if let Some(s) = s {
            s.config = Arc::new(candidate);
            s.generation = s.generation.wrapping_add(1);
            if s.selection != "auto" && s.config.profile(&s.selection).is_none() { s.selection = "auto".into(); }
            s.editor = Editor::profile(s.config.profile(&s.editor.id).or_else(|| s.config.profiles.first()));
        }
    });
    Ok(())
}

fn sync_inner(game: &Game<'_>, before: &State, m: &runtime::Module) -> Result<bool, String> {
    let edit_id = m.t("Edit profile").trim().to_ascii_lowercase();
    let selected = m.t("Server profile").trim().to_ascii_lowercase();
    let addresses = m.t("Server addresses");
    let players = m.t("Watched players");
    let command = m.p("Completion command");
    // Polling unchanged native fields must not validate/serialize the entire
    // watchlist store or rewrite its editor strings. Snapshots share its Arc.
    if edit_id == before.editor.id && selected == before.selection
        && addresses == before.editor.addresses && players == before.editor.players
        && command == before.editor.command
    {
        return Ok(false);
    }
    // Changing the editor selector loads its saved fields first. It must never
    // apply the previous profile's still-visible fields to the newly chosen id.
    if edit_id != before.editor.id {
        let p = before.config.profile(&edit_id).ok_or_else(|| format!("unknown watchlist profile {edit_id}; use .watch create <id> <address>"))?;
        with(|s| { if let Some(s) = s { s.editor = Editor::profile(Some(p)); } });
        return Ok(true);
    }
    let selected = if selected == before.selection { selected } else { selection(&before.config, &selected)? };
    if addresses != before.editor.addresses || players != before.editor.players || command != before.editor.command {
        if addresses.len() > config::MAX_EDITOR_BYTES { return Err("server address editor text is too long".into()); }
        let profile = Profile {
            id: before.editor.id.clone(),
            addresses: addresses.split(|c: char| c == ',' || c.is_ascii_whitespace()).filter(|s| !s.is_empty()).map(config::normalize_address).collect::<Result<Vec<_>, _>>()?,
            players: config::parse_players(players)?,
            command: command.to_owned(),
        };
        let mut candidate = (*before.config).clone();
        candidate.upsert(profile)?;
        if candidate != *before.config { persist(game, before, candidate)?; }
    }
    with(|s| {
        if let Some(s) = s {
            if s.selection != selected { s.generation = s.generation.wrapping_add(1); }
            s.selection = selected;
        }
    });
    Ok(true)
}

pub(super) fn sync(game: &Game<'_>) {
    runtime::ensure_loaded(game);
    ensure_loaded(game);
    let (Some(before), Some(m)) = (snapshot(), module()) else { return; };
    match sync_inner(game, &before, &m) {
        Ok(false) => {}
        Ok(true) => reload_editor(),
        Err(error) => {
            notice(error);
            // Restore all fields before runtime::save serializes a failed edit.
            reload_editor();
        }
    }
}

pub(super) struct Settings {
    pub enabled: bool,
    pub tab_checks: bool,
    pub completion_checks: bool,
    pub selection: String,
    pub config: Arc<Config>,
    pub generation: u64,
}

pub(super) fn settings(game: &Game<'_>) -> Settings {
    // Enter/Apply commits the native text editor to module state. Processing
    // here also covers Enter, which does not itself call runtime::save.
    sync(game);
    let s = snapshot();
    let m = module();
    Settings {
        enabled: m.as_ref().is_some_and(|m| m.on) && s.as_ref().is_some_and(|s| s.load_error.is_none()),
        tab_checks: m.as_ref().is_some_and(|m| m.b("TAB checks")),
        completion_checks: m.as_ref().is_some_and(|m| m.b("Completion checks")),
        selection: s.as_ref().map_or_else(|| "auto".into(), |s| s.selection.clone()),
        config: s.as_ref().map_or_else(|| Arc::new(Config::defaults()), |s| Arc::clone(&s.config)),
        generation: s.as_ref().map_or(0, |s| s.generation),
    }
}

const HELP: &[&str] = &[
    ".watch create <id> <address> - create a server profile with an empty player list",
    ".watch edit <id> - load its fields in Anti Vanish; Edit profile does not select the monitored server",
    ".watch list - list server profiles and their player counts",
    ".watch add|remove <id> <player> - change a saved player list",
    ".watch delete <id> - delete a server profile",
    ".watch auto | .watch use <id> - select by exact connected address or an explicit id",
    ".watch status - show the current connection and check state",
    "Native fields: Server profile selects auto/id; Edit profile loads saved fields; apply the address/player/command fields to save. Completion checks are opt-in.",
];

fn execute(game: &Game<'_>, args: &[&str]) -> Result<String, String> {
    let before = snapshot().ok_or("watchlists are not loaded")?;
    let sub = args.first().copied().unwrap_or("help").to_ascii_lowercase();
    match (sub.as_str(), &args[1.min(args.len())..]) {
        ("create", [id, address]) => {
            let id = config::profile_id(id)?;
            if before.config.profile(&id).is_some() { return Err(format!("profile {id} already exists")); }
            let mut candidate = (*before.config).clone();
            candidate.upsert(Profile { id: id.clone(), addresses: vec![config::normalize_address(address)?], players: Vec::new(), command: "msg".into() })?;
            persist(game, &before, candidate)?;
            with(|s| { if let Some(s) = s { s.editor = Editor::profile(s.config.profile(&id)); } });
            Ok(format!("Created {id}; its fields are loaded in Anti Vanish. Server profile remains {}.", before.selection))
        }
        ("edit", [id]) => {
            let p = before.config.profile(id).ok_or_else(|| format!("unknown watchlist profile {id}"))?;
            with(|s| { if let Some(s) = s { s.editor = Editor::profile(Some(p)); } });
            Ok(format!("Editing {} in Anti Vanish. Server profile remains {}.", p.id, before.selection))
        }
        ("add" | "remove", [id, player]) => {
            let mut p = before.config.profile(id).cloned().ok_or_else(|| format!("unknown watchlist profile {id}"))?;
            let parsed = config::parse_players(player)?;
            if parsed.len() != 1 { return Err("name one Java player".into()); }
            if sub == "add" {
                if !p.players.iter().any(|p| p.eq_ignore_ascii_case(&parsed[0])) { p.players.push(parsed[0].clone()); }
            } else { p.players.retain(|p| !p.eq_ignore_ascii_case(&parsed[0])); }
            let id = p.id.clone();
            let mut candidate = (*before.config).clone();
            candidate.upsert(p)?;
            if candidate != *before.config { persist(game, &before, candidate)?; }
            Ok(format!("{id}: {} {}.", parsed[0], if sub == "add" { "watched" } else { "removed" }))
        }
        ("delete", [id]) => {
            let mut candidate = (*before.config).clone();
            candidate.remove(id)?;
            persist(game, &before, candidate)?;
            Ok(format!("Deleted {id}."))
        }
        ("auto", []) | ("use", [_]) => {
            let selected = selection(&before.config, if sub == "auto" { "auto" } else { args[1] })?;
            with(|s| { if let Some(s) = s { if s.selection != selected { s.generation = s.generation.wrapping_add(1); } s.selection = selected.clone(); } });
            Ok(format!("Server profile: {selected}."))
        }
        _ => Err("use .watch help for server watchlist commands".into()),
    }
}

pub(super) fn command(game: &mut Game<'_>, args: &[&str]) {
    sync(game);
    let sub = args.first().copied().unwrap_or("help");
    if sub.eq_ignore_ascii_case("help") {
        for line in HELP { runtime::say(game, line); }
    } else if sub.eq_ignore_ascii_case("list") && args.len() == 1 {
        if let Some(s) = snapshot() {
            let profiles = s.config.list();
            if profiles.is_empty() { runtime::say(game, "No watchlist profiles. Use .watch create <id> <address>"); }
            for id in profiles {
                let Some(p) = s.config.profile(id) else { continue; };
                runtime::say(game, &format!("{}: {} player(s), addresses [{}], command /{}", p.id, p.players.len(), p.addresses.join(", "), p.command));
            }
        }
    } else if sub.eq_ignore_ascii_case("status") && args.len() == 1 {
        runtime::say(game, &super::sunset_watch_runtime::status());
    } else {
        let result = execute(game, args);
        reload_editor();
        if result.is_ok() { runtime::save(game); }
        runtime::say(game, &result.unwrap_or_else(|e| format!("Watchlists: {e}.")));
    }
    for message in take_notices() { runtime::say(game, &message); }
}
