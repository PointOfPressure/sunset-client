// GUI Move's GrimAC mode holds only the inventory packets. The native menu still handles every local click.
use super::remote::sunset_gui_move_remote::{self as inventory, Snapshot};
use super::sunset;
use super::{Game, Screen};
use horizon_client::net::sunset_packets as net;
use horizon_protocol::packets::play::{self, Clientbound as C, Serverbound as S};
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
const STOP_AT: usize = 128;
const MOST: usize = 1024;

#[derive(Default)]
struct Transaction {
    session: Option<Session>,
    dimension: Option<super::Dim>,
    entity: Option<i32>,
    packets: Vec<S>,
    base: Option<Snapshot>,
    closing: bool,
    quiet_move: Option<u64>,
    overflowed: bool,
    yielded: bool,
    stop_started: Option<u64>,
    wait_reported: bool,
}

thread_local! {
    static TRANSACTION: RefCell<Transaction> = RefCell::new(Transaction::default());
    static REPLAY: Cell<bool> = const { Cell::new(false) };
}

fn state<R>(run: impl FnOnce(&mut Transaction) -> R) -> R {
    TRANSACTION.with(|value| run(&mut value.borrow_mut()))
}

pub(super) fn replaying() -> bool {
    REPLAY.with(Cell::get)
}

fn enabled(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && sunset::with(|s| {
            s.module("GUI Move")
                .is_some_and(|m| m.on && m.p("Mode") == "GrimAC")
        })
}

fn own(game: &Game<'_>) -> bool {
    game.screen == Screen::Inventory && game.remote_menu().0 == 0 && !game.creative
}

pub(super) fn inventory_busy() -> bool {
    state(|s| !s.packets.is_empty() || s.closing)
}

pub(super) fn stopping() -> bool {
    state(|s| s.closing && !s.yielded)
}

fn same_session(game: &Game<'_>, s: &Transaction) -> bool {
    game.remote.as_ref().is_some_and(|r| {
        s.session
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &r.session.counters))
    }) && s.dimension == Some(game.dim)
        && s.entity == game.remote.as_ref().map(|r| r.entity_id)
}

fn attach(game: &Game<'_>, s: &mut Transaction) {
    s.session = game.remote.as_ref().map(|r| r.session.counters.clone());
    s.dimension = Some(game.dim);
    s.entity = game.remote.as_ref().map(|r| r.entity_id);
}

// Every input path calls this before predicting. An arriving slot update is rebased first, so a second click
// cannot accidentally pick up a stack from the server's old copy of the inventory.
pub(super) fn before_click(game: &mut Game<'_>) -> bool {
    if replaying() {
        return true;
    }
    if game.remote_menu().0 != 0 && inventory_busy() {
        cancel(game, "a server container replaced the inventory");
    }
    updates(game);
    if state(|s| s.closing) {
        return false;
    }
    if enabled(game) && own(game) && game.remote.is_some() {
        state(|s| {
            if s.base.is_none() {
                attach(game, s);
                s.base = Some(Snapshot::capture(game));
                net::gui_move_active(true);
            }
        });
    }
    true
}

fn begin_close(s: &mut Transaction, container: i32) {
    if !s
        .packets
        .iter()
        .any(|packet| matches!(packet, S::ContainerClose(p) if p.container == container))
    {
        s.packets
            .push(S::ContainerClose(play::ContainerClose { container }));
    }
    s.closing = true;
}

pub(super) fn outgoing(game: &Game<'_>, packet: &S) -> bool {
    if replaying() {
        return false;
    }
    match packet {
        S::ContainerClick(p) if p.container == 0 && enabled(game) && own(game) => state(|s| {
            // The native prediction hook must have captured authority before the local mutation. Never claim
            // recovery support for a raw plugin packet that did not use the menu engine.
            if s.base.is_none() {
                return false;
            }
            if s.packets.len() >= MOST {
                s.overflowed = true;
                return true;
            }
            // A native drag/quick-move batch was predicted before its first packet. If it crosses STOP_AT,
            // keep the remaining packets of that already-admitted batch ahead of the synthesized close.
            let before_close = s
                .packets
                .iter()
                .position(|p| matches!(p, S::ContainerClose(_)))
                .unwrap_or(s.packets.len());
            s.packets.insert(before_close, packet.clone());
            net::gui_move_active(s.base.is_some());
            if s.packets.len() >= STOP_AT {
                s.closing = true;
            }
            true
        }),
        S::ContainerClose(p) if p.container == 0 && (enabled(game) || inventory_busy()) => {
            state(|s| {
                if s.session.is_none() {
                    attach(game, s);
                }
                begin_close(s, p.container);
                true
            })
        }
        _ => false,
    }
}

// Called only after all transformations and every packet delay. Engine `last_input` and `was_sprinting` are
// scheduled state, which can be ahead of the server while Fake Lag or a plugin holds the packet.
pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    state(|s| {
        if !same_session(game, s) {
            return;
        }
        match packet {
            S::PlayerInput(p) if p.flags & 0x7f != 0 => s.quiet_move = None,
            S::PlayerCommand(p) if p.action == 1 => s.quiet_move = None,
            S::MovePlayerPos(_)
            | S::MovePlayerPosRot(_)
            | S::MovePlayerRot(_)
            | S::MovePlayerStatus(_) => {
                if super::sunset_combat_wire::sent_input(game) & 0x7f == 0
                    && !super::sunset_combat_wire::sent_sprinting(game)
                {
                    s.quiet_move.get_or_insert(game.ticks);
                } else {
                    s.quiet_move = None;
                }
            }
            _ => {}
        }
    });
}

fn updates(game: &mut Game<'_>) {
    if !net::gui_move_pending() {
        return;
    }
    // The new server menu must receive its own updates first. Its state hook cancels the old transaction before
    // the native handler writes those slots; an own-inventory click is never interpreted as a chest click.
    if game.remote_menu().0 != 0 {
        return;
    }
    let updates = net::gui_move_take();
    let mut failed = false;
    state(|s| {
        if !same_session(game, s) {
            return;
        }
        let Some(base) = s.base.as_mut() else {
            return;
        };
        if s.packets.is_empty() {
            inventory::restore_authority(game, base, &updates);
            *base = Snapshot::capture(game);
        } else if !inventory::rebase(game, base, &mut s.packets, &updates) {
            // An unsupported semantic operation is explicitly cancelled on the authoritative snapshot.
            // The close still synchronizes the menu; old hashes are never sent as a recovery strategy.
            s.packets
                .retain(|packet| matches!(packet, S::ContainerClose(_)));
            s.closing = true;
            failed = true;
        }
    });
    if failed {
        sunset::say(
            game,
            "GUI Move: the server changed the inventory; pending clicks could not be replayed and were cancelled.",
        );
    }
}

fn send_queued(game: &Game<'_>) {
    let packets = state(|s| std::mem::take(&mut s.packets));
    REPLAY.with(|flag| flag.set(true));
    for packet in packets {
        game.send(packet);
    }
    REPLAY.with(|flag| flag.set(false));
    state(|s| {
        s.base = None;
        s.closing = false;
        s.quiet_move = None;
        s.yielded = false;
        s.stop_started = None;
        s.wait_reported = false;
    });
    net::gui_move_active(false);
}

pub(super) fn stop(game: &mut Game<'_>) {
    state(|s| {
        if !s.packets.is_empty() {
            if own(game) {
                s.closing = true;
            } else {
                begin_close(s, 0);
            }
        }
    });
    let _ = pre_tick(game);
}

pub(super) fn packets(game: &mut Game<'_>, incoming: &[C]) {
    if incoming.iter().any(|p| matches!(p, C::PlayerPosition(_))) {
        state(|s| {
            if !s.packets.is_empty() {
                if own(game) {
                    s.closing = true;
                } else {
                    begin_close(s, 0);
                }
            }
        });
    }
    updates(game);
}

pub(super) fn incoming_before(game: &mut Game<'_>, packet: &C) {
    if !matches!(
        packet,
        C::OpenScreen(_)
            | C::MountScreenOpen(_)
            | C::ContainerClose(_)
            | C::Respawn(_)
            | C::Login(_)
            | C::Disconnect(_)
    ) {
        return;
    }
    if inventory_busy() {
        cancel(game, "the server replaced or closed the inventory context");
    } else if state(|s| s.base.is_some()) {
        updates(game);
        state(|s| s.base = None);
        net::gui_move_active(false);
    }
}

pub(super) fn pre_tick(game: &mut Game<'_>) -> bool {
    let stale = state(|s| s.session.is_some() && !same_session(game, s));
    if stale {
        let pending = inventory_busy();
        let same_connection = state(|s| {
            game.remote.as_ref().is_some_and(|r| {
                s.session
                    .as_ref()
                    .is_some_and(|old| Arc::ptr_eq(old, &r.session.counters))
            })
        });
        if same_connection {
            cancel(game, "the dimension or player life changed");
        }
        state(|s| *s = Transaction::default());
        net::gui_move_active(false);
        let _ = net::gui_move_take();
        if pending && !same_connection {
            sunset::say(
                game,
                "GUI Move: the connection ended; old inventory clicks were cancelled.",
            );
        }
        return false;
    }
    if inventory_busy() && game.remote_menu().0 != 0 {
        cancel(game, "a server container replaced the inventory");
        return false;
    }
    if inventory_busy()
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health() <= 0.0)
    {
        cancel(game, "the player died");
        return false;
    }
    updates(game);
    if state(|s| s.overflowed) {
        cancel(game, "the pending inventory batch exceeded its limit");
        return false;
    }
    let active = enabled(game);
    state(|s| {
        if !s.packets.is_empty() && (!active || !own(game)) {
            if own(game) {
                s.closing = true;
            } else {
                begin_close(s, 0);
            }
        }
        if s.packets.is_empty() && !s.closing {
            s.base = None;
            net::gui_move_active(false);
        }
    });
    let closing = state(|s| s.closing);
    if !closing {
        return false;
    }
    if state(|s| s.yielded) {
        let released = [
            winit::keyboard::KeyCode::KeyW,
            winit::keyboard::KeyCode::KeyA,
            winit::keyboard::KeyCode::KeyS,
            winit::keyboard::KeyCode::KeyD,
            winit::keyboard::KeyCode::Space,
            winit::keyboard::KeyCode::ShiftLeft,
            winit::keyboard::KeyCode::ControlLeft,
        ]
        .iter()
        .all(|key| !game.physical_keys.contains(key));
        let settled = game.riding.is_none()
            && game.player.as_ref().is_some_and(|p| {
                p.ground.on_ground
                    && !p.fluids.was_touching_water
                    && p.velocity.x.hypot(p.velocity.z) <= 0.003
            });
        if !released || !settled {
            return false;
        }
        state(|s| {
            s.yielded = false;
            s.stop_started = Some(game.ticks);
            s.quiet_move = None;
        });
    }
    state(|s| {
        s.stop_started.get_or_insert(game.ticks);
    });
    inventory::retry_idle(game);
    if let Some(player) = game.player.as_mut() {
        let _ = player.set_sprinting(false);
    }
    let ready = state(|s| {
        super::sunset_gui_move_policy::ready(
            game.ticks,
            s.quiet_move,
            super::sunset_combat_wire::sent_input(game),
            super::sunset_combat_wire::sent_sprinting(game),
            game.player
                .as_ref()
                .map_or(f64::INFINITY, |p| p.velocity.x.hypot(p.velocity.z)),
        )
    });
    // Fake Lag can still hold an old non-empty input behind the last observed idle one. Do not let a click's
    // action flush that input immediately before the click. Let the delay drain naturally while frozen.
    if ready && game.riding.is_none() && !super::sunset_lag::pending() && !net::gui_move_pending() {
        send_queued(game);
    } else if state(|s| {
        s.stop_started
            .is_some_and(|start| game.ticks.saturating_sub(start) >= 40)
    }) {
        let report = state(|s| {
            s.yielded = true;
            let report = !s.wait_reported;
            s.wait_reported = true;
            report
        });
        if report {
            sunset::say(
                game,
                "GUI Move: waiting to stop; pending clicks retained. Release movement keys on dry ground to retry.",
            );
        }
        return false;
    }
    true
}

pub(super) fn frame(game: &mut Game<'_>) {
    if game.remote.is_none() {
        let _ = pre_tick(game);
    } else {
        updates(game);
    }
}

fn cancel(game: &mut Game<'_>, reason: &str) {
    let base = state(|s| {
        s.packets.clear();
        s.closing = false;
        s.quiet_move = None;
        s.overflowed = false;
        s.yielded = false;
        s.stop_started = None;
        s.wait_reported = false;
        s.base.take()
    });
    let updates = net::gui_move_take();
    if let Some(base) = base {
        inventory::restore_authority(game, &base, &updates);
    }
    net::gui_move_active(false);
    sunset::say(
        game,
        &format!(
            "GUI Move: {reason}; pending clicks were cancelled and server inventory restored."
        ),
    );
}

// Called before the native handler applies a new server menu's first slot/content packet.
pub(super) fn menu_state(game: &mut Game<'_>, container: i32) {
    if container != 0 && inventory_busy() {
        cancel(game, "a server container replaced the inventory");
    }
}

pub(super) fn suppress_shift(game: &Game<'_>) -> bool {
    if !game.native_window()
        || !game.native_on(sunset::ID)
        || sunset::gui_open()
        || game.recipe_search_focused()
    {
        return false;
    }
    let allowed = sunset::with(|s| {
        s.module("GUI Move").is_some_and(|m| {
            m.on && game.screen == Screen::Inventory
                && !matches!(game.remote_menu().1.as_str(), "anvil" | "minecraft:anvil")
                && (m.p("Mode") != "GrimAC" || game.remote_menu().0 == 0)
        })
    });
    allowed
        && [
            winit::keyboard::KeyCode::KeyW,
            winit::keyboard::KeyCode::KeyA,
            winit::keyboard::KeyCode::KeyS,
            winit::keyboard::KeyCode::KeyD,
        ]
        .iter()
        .any(|key| game.physical_keys.contains(key))
}
