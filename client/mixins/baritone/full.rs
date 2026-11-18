// Lifecycle and movement arbitration around the supplied complete Baritone runtime.
// All process/command/pathfinding decisions remain in upstream/.
use super::{Game, Screen, baritone, baritone_command as commands, sunset_api as api,
    sunset_baritone as bounded, sunset_movement_owner as owner};
use std::cell::RefCell;

#[derive(Default)]
struct State {
    engaged: bool,
    pending_click: bool,
    lease: Option<owner::Lease>,
    world: u64,
    dimension: String,
    status: Option<String>,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

// Used by the original UI hooks as well as the process driver. The module switch
// arms jobs, while the companion/master/Panic gates all direct game hooks.
pub(super) fn present(game: &Game<'_>) -> bool {
    game.native_window() && game.native_on(baritone::ID) && game.native_on("sunset") && api::running()
}

fn enabled(game: &Game<'_>) -> bool {
    present(game) && super::sunset::with(|s| s.is_on("Baritone"))
}

fn requested_process(process: &dyn super::baritone_pathing::Process) -> bool {
    !process.is_temporary() && process.is_active()
}

fn active(game: &mut Game<'_>) -> bool {
    // InventoryPauser is always active and Backfill is autonomous. Neither is
    // an explicit user job and neither may take movement from another driver.
    baritone::initialized() && baritone::with(game, |b, _| b.core.pathing_busy()
        || b.procs.ordered().iter().any(|p| requested_process(&**p))).unwrap_or(false)
}

fn refresh_requested(game: &mut Game<'_>) {
    baritone::with(game, |b, game| {
        let baritone::Baritone { core, procs, .. } = b;
        for process in procs.ordered() {
            if !process.is_temporary() { process.refresh(core, game); }
        }
    });
}

fn waiting_for_follow(game: &mut Game<'_>) -> bool {
    baritone::initialized() && baritone::with(game, |b, _| b.procs.follow.has_request()).unwrap_or(false)
}

fn requested(game: &mut Game<'_>, line: &str) {
    // Follow's live targets are populated by refresh, not by its command.
    // Never refresh temporary processes here: InventoryPauser is always active.
    refresh_requested(game);
    let primary = active(game);
    let waiting = waiting_for_follow(game);
    let click = line.split_whitespace().next().is_some_and(|word| word.eq_ignore_ascii_case("click"));
    STATE.with_borrow_mut(|s| {
        s.engaged |= primary || waiting;
        s.pending_click |= click;
    });
}

pub(super) fn status() -> Option<String> { STATE.with_borrow(|s| s.status.clone()) }

pub(super) fn stop(game: &mut Game<'_>) {
    // Restore the actual keyboard/menu before clearing Baritone's synthetic input.
    super::baritone_input::back(game);
    if baritone::initialized() {
        baritone::with(game, |b, game| b.core.input.release(game));
        baritone::stop(game);
    }
    super::baritone_input::mend(game);
    let lease = STATE.with_borrow_mut(|s| {
        s.engaged = false; s.pending_click = false; s.status = None; s.lease.take()
    });
    if let Some(lease) = lease { owner::release(game, lease); }
}

fn arm(game: &mut Game<'_>) -> Result<(), String> {
    if !present(game) { return Err("Sunset or its Baritone companion is off.".into()); }
    if bounded::legacy_active() { bounded::stop(game, bounded::StopReason::Conflict); }
    let changed = super::sunset::with(|s| {
        let Some(module) = s.modules.iter_mut().find(|m| m.name == "Baritone") else { return false; };
        let changed = !module.on; module.on = true; changed
    });
    if changed { super::sunset::save(game); }
    Ok(())
}

fn synchronize(game: &mut Game<'_>) -> Result<(), String> {
    if !STATE.with_borrow(|s| s.engaged) { return Ok(()); }
    if !active(game) {
        baritone::with(game, |b, game| b.core.input.release(game));
        let waiting = waiting_for_follow(game);
        let lease = STATE.with_borrow_mut(|s| {
            s.engaged = waiting;
            s.status = waiting.then(|| "Baritone: Full runtime: waiting for follow targets".into());
            s.lease.take()
        });
        if let Some(lease) = lease { owner::release(game, lease); }
        return Ok(());
    }
    let held = STATE.with_borrow(|s| s.lease);
    if held.is_none() {
        if let Some(module) = owner::conflict(&super::sunset::snapshot()) {
            stop(game);
            return Err(format!("Baritone is busy with {module}; turn it off first."));
        }
        let lease = match owner::acquire(game, owner::Owner::Baritone) {
            Ok(lease) => lease,
            Err(error) => { stop(game); return Err(error); }
        };
        let world = super::sunset_seedcracker::world_id(game);
        STATE.with_borrow_mut(|s| {
            s.lease = Some(lease); s.world = world; s.dimension = game.dim.id().into();
        });
    }
    let description = baritone::with(game, |b, _| b.describe().map(|(name, _, _, text)| format!("{name}: {text}"))
        .unwrap_or_else(|| "calculating route".into())).unwrap_or_else(|| "working".into());
    STATE.with_borrow_mut(|s| s.status = Some(format!("Baritone: Full runtime: {description}")));
    Ok(())
}

pub(super) fn tick(game: &mut Game<'_>) {
    if !game.native_window() { return; }
    let lease = STATE.with_borrow(|s| s.lease);
    let changed = lease.is_some() && STATE.with_borrow(|s| s.world != super::sunset_seedcracker::world_id(game)
        || s.dimension != game.dim.id());
    let competing = lease.is_some() && (super::agent::sunset_goto::foreign_busy(game)
        || owner::conflict(&super::sunset::snapshot()).is_some());
    let invalid = !enabled(game) || game.player.is_none() || game.screen == Screen::Dead
        || game.player.as_ref().is_some_and(|p| p.health.health() <= 0.0)
        || changed || competing || lease.is_some_and(|lease| owner::current() != Some(lease));
    if invalid || bounded::legacy_active() {
        if lease.is_some() || STATE.with_borrow(|s| s.engaged || s.pending_click) { stop(game); }
        return;
    }
    if !STATE.with_borrow(|s| s.engaged) {
        // Refresh the original UI/tab/render state without running automatic
        // processes. #click may create a requested goal only after a real click.
        if baritone::initialized() { baritone::idle_tick(game); }
        if STATE.with_borrow(|s| s.pending_click) && active(game) {
            STATE.with_borrow_mut(|s| { s.engaged = true; s.pending_click = false; });
            if let Err(error) = synchronize(game) { game.native_chat(&error); }
        }
        return;
    }
    if STATE.with_borrow(|s| s.lease.is_none()) {
        // A configured follow can wait without owning movement. When a target
        // appears, acquire before the original tick can issue any input.
        refresh_requested(game);
        if let Err(error) = synchronize(game) { game.native_chat(&error); return; }
        if STATE.with_borrow(|s| s.lease.is_none()) {
            baritone::idle_tick(game);
            return;
        }
    }
    // Unlike the bounded driver, the original keeps working with chat/menus open
    // and supports Elytra. Its own motion/UI hooks implement those semantics.
    baritone::tick(game);
    if let Err(error) = synchronize(game) { game.native_chat(&error); }
}

pub(super) fn dispatch(game: &mut Game<'_>, line: &str) -> Result<(), String> {
    arm(game)?;
    baritone::with(game, |b, game| commands::run(b, game, line));
    requested(game, line);
    synchronize(game)
}

pub(super) fn chat(game: &mut Game<'_>, message: &str) -> bool {
    if !present(game) { return false; }
    // Match the original configurable prefix and doubled-prefix escape before
    // arming or cancelling any other movement owner.
    let line = baritone::with(game, |b, _| {
        let settings = &b.core.settings;
        let prefix = settings.text("prefix");
        let control = settings.flag("chatControl") || (settings.flag("chatControlAnyway") && settings.flag("prefixControl"));
        if !control || !settings.flag("prefixControl") || prefix.is_empty() { return None; }
        message.strip_prefix(prefix.as_str()).filter(|line| !line.starts_with(prefix.as_str())).map(str::to_owned)
    }).flatten();
    let Some(line) = line else { return false; };
    if let Err(error) = arm(game) { game.native_chat(&error); return true; }
    let handled = baritone::chat(game, message);
    requested(game, &line);
    if let Err(error) = synchronize(game) { game.native_chat(&error); }
    handled
}

pub(super) fn command(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    if *ran || !present(game) { return; }
    let name = command.split_whitespace().next().unwrap_or("");
    if name != commands::CLICK && !name.eq_ignore_ascii_case("baritone") && !name.eq_ignore_ascii_case("b") { return; }
    if let Err(error) = arm(game) { game.native_chat(&error); *ran = true; return; }
    baritone::command(game, command, ran);
    requested(game, command.splitn(2, char::is_whitespace).nth(1).unwrap_or(""));
    if let Err(error) = synchronize(game) { game.native_chat(&error); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::baritone_pathing::Process;

    #[test]
    fn baritone_full_temporary_processes_cannot_engage_movement() {
        let pauser = super::super::baritone_p_backfill::InventoryPauserProcess;
        let pause = super::super::baritone_p_pause::PauseProcess { paused: true };
        assert!(pauser.is_active() && pause.is_active());
        assert!(!requested_process(&pauser));
        assert!(!requested_process(&pause));
        let mut goal = super::super::baritone_p_goal::CustomGoalProcess::new();
        assert!(!requested_process(&goal));
        goal.set_goal_and_path(super::super::baritone_goals::rc(super::super::baritone_goals::GoalBlock([3, 64, 5])));
        assert!(requested_process(&goal));
    }

    #[test]
    fn baritone_full_follow_request_survives_empty_target_snapshot() {
        use super::super::baritone_p_follow::{Filter, FollowProcess};
        let mut follow = FollowProcess::new();
        assert!(!follow.has_request());
        follow.follow(Filter::Players);
        // A command configures the filter before the first game-world refresh.
        assert!(follow.has_request());
        assert!(!follow.is_active());
        assert!(!requested_process(&follow));
        follow.cancel();
        assert!(!follow.has_request());
    }
}
