// Server watchlists: exact profile selection and named evidence, with no automatic actions.
use super::{
    Game, sunset as runtime,
    sunset_watch_state::{self as model, Alert, WatchState},
    sunset_watch_ui as ui,
};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct State {
    address: Option<String>,
    local: bool,
    signature: Option<(u64, String, bool, bool)>,
    command: Option<String>,
    engine: WatchState,
    next_id: Option<i32>,
    lowest_id: Option<i32>,
    pending_range: Option<(i32, usize, usize)>,
    last_poll: Option<u64>,
    status: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            address: None,
            local: true,
            signature: None,
            command: None,
            engine: WatchState::default(),
            next_id: Some(super::sunset_watch_wire::FIRST_ID),
            lowest_id: None,
            pending_range: None,
            last_poll: None,
            status: "Anti Vanish: no multiplayer connection.".into(),
        }
    }
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
fn now() -> u64 {
    START
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE
        .get_or_init(|| Mutex::new(State::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner()))
}

fn reset(state: &mut State) {
    state.engine.reset();
    state.signature = None;
    state.command = None;
    state.pending_range = None;
}

pub(super) fn connecting(game: &Game<'_>, address: &str, local: bool) {
    if !game.native_window() {
        return;
    }
    with(|state| {
        reset(state);
        state.address = Some(address.to_owned());
        state.local = local;
        state.last_poll = None;
        state.status = "Anti Vanish: connecting; no evidence yet.".into();
    });
}

pub(super) fn leaving(game: &Game<'_>) {
    if !game.native_window() {
        return;
    }
    with(|state| {
        reset(state);
        state.address = None;
        state.local = true;
        state.last_poll = None;
        state.status = "Anti Vanish: no multiplayer connection.".into();
    });
}

pub(super) fn dimension(game: &Game<'_>) {
    if game.native_window() {
        with(|state| {
            reset(state);
            state.last_poll = None;
        });
    }
}

pub(super) fn status() -> String {
    let time = now();
    with(|state| {
        let mut text = state.status.clone();
        if state.signature.is_some() && state.command.is_some() {
            let rows = state.engine.rows(time);
            for row in rows.iter().take(4) {
                let evidence = row.evidence.map(|kind| kind.label()).unwrap_or_else(|| {
                    if row.listed && row.spectator {
                        "listed; spectator"
                    } else if row.listed {
                        "listed"
                    } else if row.present_in_roster {
                        "unlisted roster entry"
                    } else if row.uuid.is_some() {
                        "absent; unconfirmed"
                    } else {
                        "not observed"
                    }
                });
                text.push_str(&format!(" {}: {evidence}.", row.name));
            }
            if rows.len() > 4 {
                text.push_str(&format!(" {} more watched.", rows.len() - 4));
            }
        }
        text
    })
}

fn configure(state: &mut State, settings: &ui::Settings, connected: bool) -> Option<String> {
    if !settings.enabled || !connected || state.local || state.address.is_none() {
        reset(state);
        state.status = if !settings.enabled {
            "Anti Vanish: OFF."
        } else {
            "Anti Vanish: no multiplayer connection."
        }
        .into();
        return None;
    }
    let signature = (
        settings.generation,
        settings.selection.clone(),
        settings.tab_checks,
        settings.completion_checks,
    );
    if state.signature.as_ref() == Some(&signature) {
        return state.command.clone();
    }
    let profile = match settings
        .config
        .resolve(&settings.selection, state.address.as_deref())
    {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            reset(state);
            state.status =
                "Anti Vanish: no matching server profile; use .watch create or select a profile."
                    .into();
            return None;
        }
        Err(error) => {
            reset(state);
            state.status = format!("Anti Vanish: {error}.");
            return None;
        }
    };
    reset(state);
    state.signature = Some(signature);
    state.command = Some(profile.command.clone());
    state.engine.configure(
        model::Config {
            enabled: true,
            passive_tab: settings.tab_checks,
            completion_probe: settings.completion_checks,
        },
        &profile.players,
    );
    state.status = format!(
        "Anti Vanish: {} on {}; {} watched; TAB {}; completion {}.",
        profile.id,
        state.address.as_deref().unwrap_or(""),
        profile.players.len(),
        if settings.tab_checks { "ON" } else { "OFF" },
        if settings.completion_checks {
            "ON"
        } else {
            "OFF"
        }
    );
    Some(profile.command.clone())
}

fn announce(game: &mut Game<'_>, alerts: Vec<Alert>) {
    let mut strongest = std::collections::BTreeMap::<String, Alert>::new();
    for alert in alerts {
        let key = alert.uuid.map_or_else(
            || alert.name.to_ascii_lowercase(),
            |id| format!("{id:032x}"),
        );
        if strongest
            .get(&key)
            .is_none_or(|old| alert.evidence > old.evidence)
        {
            strongest.insert(key, alert);
        }
    }
    for alert in strongest.into_values().take(model::MAX_WATCHED) {
        runtime::say(
            game,
            &format!("Watch {}: {}.", alert.name, alert.evidence.label()),
        );
    }
}

pub(super) fn frame(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    if !game.native_on(runtime::ID) {
        with(|state| {
            reset(state);
            state.status = "Anti Vanish: mod disabled.".into();
        });
        return;
    }
    let time = now();
    if with(|state| {
        if state
            .last_poll
            .is_some_and(|last| time.saturating_sub(last) < 250)
        {
            true
        } else {
            state.last_poll = Some(time);
            false
        }
    }) {
        return;
    }
    let settings = ui::settings(game);
    for notice in ui::take_notices() {
        runtime::say(game, &notice);
    }
    let connected = game.remote.as_ref().is_some_and(|r| r.logged_in)
        && game.player.is_some()
        && game.integrated.is_none();
    let command = with(|state| configure(state, &settings, connected));
    let Some(command) = command else {
        return;
    };
    let Some(snapshot) = super::remote::sunset_watch_remote::snapshot(game) else {
        with(|state| {
            reset(state);
            state.status = "Anti Vanish: player snapshot unavailable; detection paused.".into();
        });
        return;
    };
    let can_probe = game.native_playing() && !runtime::gui_open() && game.focused;
    let (alerts, request) = with(|state| {
        let mut alerts = state.engine.update_roster(time, &snapshot.roster, &[]);
        alerts.extend(
            state
                .engine
                .update_nearby(time, snapshot.local, &snapshot.nearby),
        );
        let request = if can_probe {
            state.engine.probe_candidate(time).and_then(|name| {
                let id = state.next_id?;
                if !state.engine.probe_sent(time, id, &name) {
                    return None;
                }
                state.next_id = id.checked_sub(1);
                state.lowest_id = Some(id);
                state.pending_range = Some((id, command.len() + 2, name.len()));
                Some(super::sunset_watch_wire::request(id, &command, &name))
            })
        } else {
            None
        };
        (alerts, request)
    });
    announce(game, alerts);
    if let Some(data) = request {
        game.send(horizon_protocol::packets::play::Serverbound::Unknown(
            horizon_protocol::ids::play::serverbound::COMMAND_SUGGESTION,
            data,
        ));
    }
}

// True only for IDs this module actually issued. Such replies never reach chat's suggestion widget.
pub(super) fn suggestions(game: &mut Game<'_>, data: &[u8]) -> bool {
    let Some(id) = super::sunset_watch_wire::reply_id(data) else {
        return false;
    };
    let owned = with(|state| {
        state
            .lowest_id
            .is_some_and(|lowest| id >= lowest && id <= super::sunset_watch_wire::FIRST_ID)
    });
    if !owned {
        return false;
    }
    if with(|state| state.pending_range.is_none_or(|pending| pending.0 != id)) {
        return true;
    }
    if !game.native_window() || !game.native_on(runtime::ID) {
        with(reset);
        return true;
    }
    let time = now();
    let settings = ui::settings(game);
    let connected = game.remote.as_ref().is_some_and(|r| r.logged_in)
        && game.player.is_some()
        && game.integrated.is_none();
    if with(|state| configure(state, &settings, connected)).is_none() {
        return true;
    }
    let Some(snapshot) = super::remote::sunset_watch_remote::snapshot(game) else {
        with(reset);
        return true;
    };
    let alerts = with(|state| {
        let mut alerts = state.engine.update_roster(time, &snapshot.roster, &[]);
        if let Some(expected) = state.pending_range.filter(|r| r.0 == id) {
            state.pending_range = None;
            let names = super::sunset_watch_wire::suggestions(data, expected).unwrap_or_default();
            alerts.extend(state.engine.completion_reply(time, id, &names));
        }
        alerts
    });
    announce(game, alerts);
    true
}
