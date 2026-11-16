// Sunset controls the optional Baritone companion. All pathfinding and mining policy stays in its replaceable DLL.
use super::{Game, Screen, sunset_baritone_actions as actions, sunset_baritone_targets as targets,
    sunset_movement_owner as owner, sunset_ore_sim as ores, sunset_api as api};
use super::sunset::{Category, Module};
use serde_json::{Value, json};
use std::{collections::{HashMap, HashSet}, panic::{AssertUnwindSafe, catch_unwind}, sync::Mutex};

const NAME: &str = "Baritone";
const MAX_BYTES: usize = 4 * 1024 * 1024;

pub(super) fn defaults() -> Module {
    Module::new(NAME, Category::World, "Built-in Baritone navigation, building, farming and mining. Start with #help or .b help.")
        .pick("Source", &["Visible ores", "Ore Sim"]).text("Ores", "diamond")
        .num("Min Y", -64.0, -4096.0, 4095.0, 1.0).num("Max Y", 319.0, -4096.0, 4095.0, 1.0)
        .num("Radius", 128.0, 8.0, 256.0, 1.0).num("Max targets", 64.0, 1.0, 4096.0, 1.0)
        .bool("Allow break", true).bool("Allow place", false).bool("Sprint", true)
        .bool("Path", true).bool("Goal", true).bool("Targets", true)
        .colour("Path color", "Sunset Orange").colour("Goal color", "Gold").colour("Target color", "Sunset Orange")
        .pick("Shape mode", &["Both", "Lines", "Sides"]).num("Fill opacity", 0.156863, 0.0, 1.0, 0.01)
        .num("Line width", 1.0, 0.5, 4.0, 0.25).num("Render cap", 256.0, 0.0, 2048.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StopReason {
    User, Disabled, MasterOff, Panic, Disconnect, Death, Dimension,
    SourceChanged, InputOverride, Screen, Conflict, EngineError,
}
#[derive(Clone, Debug)]
pub(super) struct DrawSnapshot {
    pub world: u64, pub dimension: &'static str, pub job: u64,
    pub path: Vec<[i32; 3]>, pub next_path: Vec<[i32; 3]>,
    pub goal: Option<[i32; 3]>, pub target: Option<[i32; 3]>,
    pub breaking: Vec<[i32; 3]>, pub placing: Vec<[i32; 3]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Status, Help, Stop, Off, Mine, Targets(usize), Goto([i32; 3]), Visible(String), Predicted(Option<String>),
}
pub(super) fn parse(args: &[&str]) -> Result<Command, String> {
    match args {
        [] | ["status"] => Ok(Command::Status), ["help"] => Ok(Command::Help),
        ["stop"] => Ok(Command::Stop), ["off"] => Ok(Command::Off),
        ["mine"] => Ok(Command::Mine),
        ["targets"] => Ok(Command::Targets(8)),
        ["targets", limit] => limit.parse::<usize>().ok().filter(|&n| n > 0)
            .map(|n| Command::Targets(n.min(64))).ok_or_else(|| "Use .b targets [1..64].".into()),
        ["goto", x, y, z] => {
            let at = [x, y, z].map(|v| v.parse::<i32>());
            let [Ok(x), Ok(y), Ok(z)] = at else { return Err("Use .b goto <x> <y> <z> with whole block coordinates.".into()); };
            let pos = [x, y, z];
            if !valid_pos(pos) { return Err("That is outside the world.".into()); }
            Ok(Command::Goto(pos))
        }
        ["mine", "oresim"] => Ok(Command::Predicted(None)),
        ["mine", "oresim", kinds] => { targets::parse_kinds(kinds)?; Ok(Command::Predicted(Some((*kinds).into()))) }
        ["mine", blocks] => {
            if blocks.split(',').any(|b| b.is_empty() || !b.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_:/.-".contains(c))) {
                return Err("Use .b mine <block[,block...]> or .b mine oresim [ore[,ore...]].".into());
            }
            Ok(Command::Visible((*blocks).into()))
        }
        _ => Err("Use .b status|goto <x> <y> <z>|mine <blocks>|mine oresim [ores]|stop|off|targets [limit]|help.".into()),
    }
}

#[derive(Clone)]
enum Source { Goto([i32; 3]), Visible(Vec<String>), Predicted }
struct Job {
    world: u64, dimension: &'static str, generation: u64, lease: owner::Lease,
    source: Source, query: ores::MiningQuery, settings: Module,
    ledger: Option<targets::TargetLedger>, offered: HashMap<[i32; 3], u8>, verified: HashSet<[i32; 3]>,
    revision: u64, refill: Option<u64>, last_tick: Option<u64>, view: (f32, f32),
    block_key: (u64, u64), block_revision: u64,
    selected: usize,
}
struct Controller {
    generation: u64, job: Option<Job>, status: String, counts: [u64; 4],
    draw: Option<DrawSnapshot>, available: bool,
}
static CONTROLLER: Mutex<Option<Controller>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut Controller) -> R) -> R {
    let mut state = CONTROLLER.lock().unwrap_or_else(|e| e.into_inner());
    f(state.get_or_insert_with(|| Controller { generation: 0, job: None, status: "Unavailable".into(),
        counts: [0; 4], draw: None, available: false }))
}

fn valid_pos(pos: [i32; 3]) -> bool {
    pos[0].unsigned_abs() <= 29_999_983 && pos[2].unsigned_abs() <= 29_999_983 && (-4096..=4095).contains(&pos[1])
}
fn feet(game: &Game<'_>) -> Option<[f64; 3]> {
    game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z])
}
fn input_available(game: &Game<'_>) -> bool {
    game.native_playing() && !game.screen_over_playing() && game.sleeping.is_none() && !super::sunset::gui_open()
}
fn envelope(job: &Job) -> Value {
    json!({"schema":1,"world":job.world.to_string(),"dimension":job.dimension,
        "job_generation":job.generation.to_string(),"lease_generation":job.lease.generation.to_string()})
}
fn invoke(service: api::BaritoneServiceV1, game: &Game<'_>, op: u32, request: &Value) -> Result<Value, String> {
    let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES { return Err("Baritone request exceeds the protocol bound.".into()); }
    let output = catch_unwind(AssertUnwindSafe(|| (service.call)(game, op, &bytes)))
        .map_err(|_| "Baritone companion panicked.".to_owned())??;
    if output.len() > MAX_BYTES { return Err("Baritone response exceeds the protocol bound.".into()); }
    let value: Value = serde_json::from_slice(&output).map_err(|e| format!("Invalid Baritone response: {e}"))?;
    if value["schema"].as_u64() != Some(1) { return Err("Unsupported Baritone response schema.".into()); }
    Ok(value)
}
fn ready(game: &Game<'_>) -> Option<api::BaritoneServiceV1> {
    api::baritone_service().filter(|s| catch_unwind(AssertUnwindSafe(|| (s.available)(game))).unwrap_or(false))
}

pub(super) fn execution_changed(a: &Module, b: &Module) -> bool {
    ["Source", "Ores", "Min Y", "Max Y", "Radius", "Max targets", "Allow break", "Allow place", "Sprint"]
        .iter().any(|name| a.setting(name) != b.setting(name))
}

pub(super) fn stop(game: &mut Game<'_>, reason: StopReason) {
    if !game.native_window() { return; }
    if let Some(full) = api::baritone_service().and_then(|s| s.full) { (full.stop)(game); }
    let world = (reason == StopReason::SourceChanged).then(|| super::sunset_seedcracker::world_id(game));
    let (lease, stale_world) = state(|c| {
        c.generation = c.generation.wrapping_add(1).max(1);
        let stale = world.is_some_and(|world| c.job.as_ref().is_some_and(|j| j.world != world || j.dimension != game.dim.id()));
        let lease = c.job.take().map(|j| j.lease);
        c.draw = None;
        c.status = if !c.available { "Unavailable".into() }
            else if lease.is_some() && !matches!(reason, StopReason::User | StopReason::Disabled) { format!("Stopped: {reason:?}") }
            else { "Idle".into() };
        (lease, stale)
    });
    let lease = lease.or_else(|| owner::current().filter(|l| l.owner == owner::Owner::Baritone));
    if let Some(lease) = lease {
        if stale_world || matches!(reason, StopReason::Disconnect | StopReason::Death | StopReason::Dimension)
            || super::agent::sunset_goto::foreign_busy(game) {
            actions::discard(game, lease);
        } else { actions::release(game, lease); }
        owner::release(game, lease);
    }
    // Copy out before calling; cancellation is legal after Panic gates ordinary plugin callbacks.
    if let Some(service) = api::baritone_service() {
        let _ = catch_unwind(AssertUnwindSafe(service.cancel));
    }
}

pub(super) fn legacy_active() -> bool { state(|c| c.job.is_some()) }

pub(super) fn maintain(game: &mut Game<'_>) {
    if !game.native_window() { return; }
    let available = ready(game).is_some();
    let module = super::sunset::with(|s| s.module(NAME).cloned());
    let active = state(|c| {
        let availability_changed = c.available != available;
        c.available = available;
        if c.job.is_none() && (availability_changed || c.status == "Unavailable") {
            c.status = if available { "Idle" } else { "Unavailable" }.into();
        }
        c.job.as_ref().map(|j| (j.world, j.dimension, j.lease, j.view, j.selected,
            module.as_ref().is_none_or(|m| execution_changed(m, &j.settings))))
    });
    let Some((world, dim, lease, view, selected, changed)) = active else { return; };
    let reason = if !super::sunset::running() { Some(StopReason::Panic) }
        else if !game.native_on("sunset") { Some(StopReason::MasterOff) }
        else if game.player.is_none() { Some(StopReason::Disconnect) }
        else if game.screen == Screen::Dead || game.player.as_ref().is_some_and(|p| p.health.health() <= 0.0) { Some(StopReason::Death) }
        else if game.spectator() || game.riding.is_some() || game.player.as_ref().is_some_and(|p| p.flying || p.fall_flying) { Some(StopReason::Conflict) }
        else if game.dim.id() != dim { Some(StopReason::Dimension) }
        else if super::sunset_seedcracker::world_id(game) != world { Some(StopReason::SourceChanged) }
        else if module.as_ref().is_none_or(|m| !m.on) { Some(StopReason::Disabled) }
        else if changed { Some(StopReason::SourceChanged) }
        else if !available { Some(StopReason::EngineError) }
        else if !input_available(game) { Some(StopReason::Screen) }
        else if owner::current() != Some(lease) || owner::foreign_keys(game) || super::agent::sunset_goto::foreign_busy(game)
            || super::sunset::with(|s| owner::conflict(&s.modules).is_some())
            || super::sunset_combat_control::owner().is_some() || super::sunset::strike_waiting()
            || super::sunset_melee::due_waiting() || super::sunset_gui_move::inventory_busy() { Some(StopReason::Conflict) }
        else {
            let held = super::sunset::native_held_input(game);
            (owner::MOVE_KEYS.iter().any(|k| held.0.contains(k)) || !held.1.is_empty() || actions::slot_changed(game)
                || (game.yaw, game.pitch) != view || game.selected != selected || !game.hotbar_presses.is_empty())
                .then_some(StopReason::InputOverride)
        };
    if let Some(reason) = reason { stop(game, reason); }
}

pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_window() { return; }
    let actions = ["key.forward", "key.back", "key.left", "key.right", "key.jump", "key.sneak", "key.sprint", "key.attack", "key.use",
        "key.pickItem", "key.drop", "key.swapOffhand", "key.inventory", "key.hotbar.1", "key.hotbar.2", "key.hotbar.3",
        "key.hotbar.4", "key.hotbar.5", "key.hotbar.6", "key.hotbar.7", "key.hotbar.8", "key.hotbar.9"];
    if actions.iter().any(|a| game.settings.keys.get(a) == key) && state(|c| c.job.is_some()) {
        stop(game, StopReason::InputOverride);
    }
}

pub(super) fn source_changed(game: &mut Game<'_>) {
    if state(|c| c.job.as_ref().is_some_and(|j| matches!(j.source, Source::Predicted))) {
        stop(game, StopReason::SourceChanged);
    }
}

fn query(game: &Game<'_>, module: &Module, kinds: u16) -> Result<ores::MiningQuery, String> {
    targets::validated_query(feet(game).ok_or("No player.")?, kinds, module.n("Min Y") as i32, module.n("Max Y") as i32,
        module.n("Radius"), module.n("Max targets") as usize, game.world.shape.min_y, game.world.shape.max_y() - 1)
}

fn configure(game: &Game<'_>, job: &Job, service: api::BaritoneServiceV1) -> Result<(), String> {
    let m = &job.settings;
    invoke(service, game, 1, &json!({"schema":1,"anchor":job.query.anchor,"radius":job.query.radius,
        "min_y":job.query.min_y,"max_y":job.query.max_y,"max_targets":job.query.limit,
        "settings":{"allowBreak":m.b("Allow break"),"allowPlace":m.b("Allow place"),"allowSprint":m.b("Sprint"),
            "allowParkour":false,"maxFallHeightNoWater":3,
            "minYLevelWhileMining":job.query.min_y - game.world.shape.min_y,"maxYLevelWhileMining":job.query.max_y,
            "mineMaxOreLocationsCount":job.query.limit}}))?;
    Ok(())
}

fn start(game: &mut Game<'_>, command: Command) -> Result<String, String> {
    if !game.native_window() || !super::sunset::running() || !game.native_on("sunset") { return Err("Sunset is off.".into()); }
    if !input_available(game) { return Err("Start Baritone in a world with the menu closed and the player awake.".into()); }
    if game.player.as_ref().is_none_or(|p| p.health.health() <= 0.0) { return Err("A living player is required.".into()); }
    if game.spectator() || game.riding.is_some() || game.player.as_ref().is_some_and(|p| p.flying || p.fall_flying) {
        return Err("Baritone V1 uses ordinary walking/swimming; land and leave your vehicle first.".into());
    }
    let service = ready(game).ok_or("Baritone Unavailable: install/enable the Sunset Baritone companion and its compatible engine DLL.")?;
    if game.settings.mods.installed.iter().any(|m| m.id == "baritone") && game.native_on("baritone") {
        return Err("The standalone Baritone mod owns another input driver; turn it off first.".into());
    }
    let mods = super::sunset::snapshot();
    if let Some(name) = owner::conflict(&mods) { return Err(format!("Baritone is busy with {name}; turn it off first.")); }
    if super::sunset_combat_control::owner().is_some() || super::sunset::strike_waiting()
        || super::sunset_melee::due_waiting() || super::sunset_gui_move::inventory_busy()
    { return Err("Combat or inventory automation owns the hand; stop it first.".into()); }
    owner::can_acquire(game)?;
    let module = mods.iter().find(|m| m.name == NAME).cloned().ok_or("Baritone settings are missing.")?;
    let command = match command {
        Command::Mine if module.p("Source") == "Ore Sim" => Command::Predicted(None),
        Command::Mine => Command::Visible(module.t("Ores").to_owned()),
        command => command,
    };
    let (source, mask) = match command {
        Command::Goto(pos) => {
            if pos[1] < game.world.shape.min_y || pos[1] >= game.world.shape.max_y() { return Err("Goal is outside this dimension's build range.".into()); }
            (Source::Goto(pos), 0x03ff)
        }
        Command::Visible(text) => {
            let mut blocks = Vec::new();
            for block in text.split(',') {
                if (0..10).any(|kind| targets::kind_name(kind) == Some(block)) {
                    let mask = targets::parse_kinds(block)?;
                    // Kind aliases include their exact normal/deepslate/nether ore family.
                    for name in ore_names(mask) { if !blocks.contains(&name) { blocks.push(name); } }
                } else {
                    let block = if block.contains(':') { block.to_owned() } else { format!("minecraft:{block}") };
                    if game.pack.assets.registry.default_state(&block).is_none() { return Err(format!("Unknown block '{block}'.")); }
                    if !blocks.contains(&block) { blocks.push(block); }
                }
            }
            (Source::Visible(blocks), 0x03ff)
        }
        Command::Predicted(override_kinds) => {
            if !super::sunset::with(|s| s.is_on("Ore Sim")) { return Err("Turn Ore Sim on first. Baritone does not change its seed or database settings.".into()); }
            (Source::Predicted, targets::parse_kinds(override_kinds.as_deref().unwrap_or(module.t("Ores")))?)
        }
        _ => return Err("No Baritone job was specified.".into()),
    };
    let query = if matches!(source, Source::Goto(_)) {
        targets::validated_query(feet(game).ok_or("No player.")?, mask, game.world.shape.min_y, game.world.shape.max_y() - 1,
            module.n("Radius"), module.n("Max targets") as usize, game.world.shape.min_y, game.world.shape.max_y() - 1)?
    } else { query(game, &module, mask)? };
    if let Source::Goto(pos) = source {
        let distance = (0..3).map(|i| (f64::from(pos[i]) + 0.5 - query.anchor[i]).powi(2)).sum::<f64>();
        if distance > query.radius.powi(2) { return Err(format!("Goal exceeds the {:.0}-block Baritone radius.", query.radius)); }
    }
    // Finish a number-key selection issued before this valid start, before the first tool observation.
    // Subsequent queued selections are manual overrides and cancel in maintain().
    game.select_queued_hotbar();
    let world = super::sunset_seedcracker::world_id(game);
    stop(game, StopReason::User);
    let lease = owner::acquire(game, owner::Owner::Baritone)?;
    let generation = state(|c| { c.generation = c.generation.wrapping_add(1).max(1); c.generation });
    let mut job = Job { world, dimension: game.dim.id(), generation, lease, source, query,
        settings: module, ledger: None, offered: HashMap::new(), verified: HashSet::new(), revision: 0, refill: None, last_tick: None,
        view: (game.yaw, game.pitch), block_key: (game.world.revision, game.world.chunks.membership_revision()), block_revision: 1,
        selected: game.selected };
    let result = (|| {
        configure(game, &job, service)?;
        let mut request = envelope(&job);
        match &job.source {
            Source::Goto(goal) => { request["goal"] = json!(goal); invoke(service, game, 2, &request)?; }
            Source::Visible(blocks) => { request["blocks"] = json!(blocks); invoke(service, game, 3, &request)?; }
            Source::Predicted => { refill(game, &mut job, service, true)?; }
        }
        Ok::<(), String>(())
    })();
    if let Err(e) = result {
        actions::release(game, lease); owner::release(game, lease);
        let _ = catch_unwind(AssertUnwindSafe(service.cancel));
        return Err(e);
    }
    let predicted = matches!(job.source, Source::Predicted);
    state(|c| { c.job = Some(job); c.draw = None; c.counts = [0; 4]; c.available = true;
        c.status = if predicted { "Waiting for seed/replay" } else { "Pathing" }.into(); });
    super::sunset::with(|s| { if let Some(m) = s.modules.iter_mut().find(|m| m.name == NAME) { m.on = true; } });
    super::sunset::save(game);
    Ok("Baritone started. .b status shows progress; .b stop releases its controls.".into())
}

fn ore_names(mask: u16) -> Vec<String> {
    let mut names = Vec::new();
    for kind in 0..10 {
        if mask & (1 << kind) == 0 { continue; }
        let name = targets::kind_name(kind).unwrap_or("");
        if kind < 8 {
            names.push(format!("minecraft:{name}_ore")); names.push(format!("minecraft:deepslate_{name}_ore"));
            if kind == 5 { names.push("minecraft:nether_gold_ore".into()); }
        } else { names.push(if kind == 8 { "minecraft:nether_quartz_ore" } else { "minecraft:ancient_debris" }.into()); }
    }
    names
}

fn refill(game: &Game<'_>, job: &mut Job, service: api::BaritoneServiceV1, force: bool) -> Result<(), String> {
    // Validate the source even between refills, without rescanning thousands of
    // predictions just to discover that the last offered batch is unchanged.
    let header = match ores::mining_header(game) {
        Ok(header) => header,
        Err(ores::PredictionUnavailable::Disabled | ores::PredictionUnavailable::StaleWorld) => return Err("Ore Sim source is disabled or stale.".into()),
        Err(ores::PredictionUnavailable::NoSeed | ores::PredictionUnavailable::Replaying) => {
            if job.ledger.is_some() { return Err("Ore Sim replay changed.".into()); }
            return Ok(());
        }
    };
    if let Some(ledger) = &job.ledger {
        if ledger.key() != header.key { return Err("Ore Sim source key changed; start a new job.".into()); }
    }
    let due = force || job.refill.is_none_or(|tick| game.ticks.saturating_sub(tick) >= 5);
    if !due || !force && job.revision == header.revision && job.refill.is_some() { return Ok(()); }
    let empty = HashSet::new();
    let excluded = job.ledger.as_ref().map_or(&empty, |l| l.exclusions());
    let snapshot = match ores::mining_snapshot(game, &job.query, excluded) {
        Ok(s) => s,
        Err(ores::PredictionUnavailable::Disabled | ores::PredictionUnavailable::StaleWorld) => return Err("Ore Sim source is disabled or stale.".into()),
        Err(ores::PredictionUnavailable::NoSeed | ores::PredictionUnavailable::Replaying) => {
            if job.ledger.is_some() { return Err("Ore Sim replay changed.".into()); }
            return Ok(());
        }
    };
    if let Some(ledger) = &job.ledger {
        if ledger.key() != snapshot.key { return Err("Ore Sim source key changed; start a new job.".into()); }
    } else { job.ledger = Some(targets::TargetLedger::new(snapshot.key, job.query.limit)); }
    let ledger = job.ledger.as_mut().ok_or("Missing target ledger.")?;
    if !force && job.revision == snapshot.revision && job.refill.is_some() { return Ok(()); }
    let feet = feet(game).ok_or("No player.")?;
    let chosen = targets::ordered_candidates(&snapshot, &job.query, feet, ledger.exclusions(), ledger.remaining());
    for ore in chosen { if ledger.attempt(ore.pos) { job.offered.insert(ore.pos, ore.kind); } }
    let mut targets: Vec<_> = job.offered.iter().map(|(&pos, &kind)| (pos, kind)).collect();
    let distance = |pos: [i32; 3]| (0..3).map(|i| (f64::from(pos[i]) + 0.5 - feet[i]).powi(2)).sum::<f64>();
    targets.sort_by(|(a, ak), (b, bk)| distance(*a).total_cmp(&distance(*b)).then(a.cmp(b)).then(ak.cmp(bk)));
    let mut request = envelope(job);
    request["source"] = json!({"world":snapshot.key.world.to_string(),"replay":snapshot.key.replay.to_string(),"dimension":snapshot.key.dimension});
    request["revision"] = json!(snapshot.revision.to_string()); request["pending"] = json!(snapshot.pending);
    request["targets"] = json!(targets.iter().map(|(pos, kind)| json!({"pos":pos,"kind":kind})).collect::<Vec<_>>());
    invoke(service, game, 4, &request)?;
    job.revision = snapshot.revision; job.refill = Some(game.ticks);
    Ok(())
}

fn frame(game: &Game<'_>, job: &Job) -> Result<Value, String> {
    let p = game.player.as_ref().ok_or("No player.")?;
    let mut request = envelope(job);
    request["tick"] = json!(game.ticks.to_string()); request["registry_epoch"] = json!(job.world.to_string());
    request["block_revision"] = json!(job.block_revision.to_string());
    request["settings_revision"] = json!(job.generation.to_string()); request["target_revision"] = json!(job.revision.to_string());
    request["input_available"] = json!(true);
    request["player"] = json!({"position":[p.position.x,p.position.y,p.position.z],"velocity":[p.velocity.x,p.velocity.y,p.velocity.z],
        "yaw":game.yaw,"pitch":game.pitch,"eye_height":p.dimensions.eye_height,"on_ground":p.ground.on_ground,
        "in_water":p.fluids.was_touching_water,"in_lava":p.is_in_lava(),"sprinting":p.sprinting,
        "swimming":p.swimming,"under_water":p.fluids.was_eye_in_water,"crouching":p.local_crouching,
        "air":p.air,"max_air":300,"on_climbable":p.on_climbable,"movement_speed":p.speed,
        "horizontal_collision":p.collision.horizontal,"food":p.health.food.food,"reach":game.block_reach});
    request["selected_slot"] = json!(game.selected);
    request["hotbar"] = json!((0..9).filter_map(|slot| game.inventory.get(slot).and_then(Option::as_ref)
        .map(|item| json!({"slot":slot,"name":item.item.as_ref(),"count":item.count}))).collect::<Vec<_>>());
    request["min_y"] = json!(game.world.shape.min_y); request["max_y"] = json!(game.world.shape.max_y() - 1);
    Ok(request)
}

pub(super) fn response_matches(value: &Value, world: u64, dim: &str, job: u64, lease: u64, tick: u64) -> bool {
    value["schema"].as_u64() == Some(1) && value["dimension"].as_str() == Some(dim)
        && [("world", world), ("job_generation", job), ("lease_generation", lease), ("tick", tick)]
            .into_iter().all(|(k, v)| value[k].as_str() == Some(v.to_string().as_str()))
}
fn position(value: &Value) -> Option<[i32; 3]> {
    serde_json::from_value::<[i32; 3]>(value.clone()).ok().filter(|&p| valid_pos(p))
}
fn positions(value: &Value, cap: usize) -> Vec<[i32; 3]> {
    value.as_array().into_iter().flatten().take(cap).filter_map(position).collect()
}
fn update(game: &Game<'_>, job: &mut Job, value: &Value) -> Result<(String, [u64; 4], DrawSnapshot), String> {
    if let Some(events) = value["events"].as_array() {
        if events.len() > 4096 { return Err("Too many Baritone terminal events.".into()); }
        for event in events {
            let pos = position(&event["pos"]).ok_or("Invalid target event position.")?;
            let Some(ledger) = job.ledger.as_mut() else { continue; };
            let key = ledger.key();
            let kind = job.offered.get(&pos).copied().ok_or("Unknown Baritone target event.")?;
            if event["source"]["world"].as_str() != Some(key.world.to_string().as_str())
                || event["source"]["replay"].as_str() != Some(key.replay.to_string().as_str())
                || event["source"]["dimension"].as_str() != Some(key.dimension)
                || Some(u64::from(kind)) != event["kind"].as_u64() { return Err("Stale or unknown Baritone target event.".into()); }
            let outcome = match event["outcome"].as_str() {
                Some("mined") => targets::Outcome::Mined, Some("mismatch") => targets::Outcome::Mismatch,
                Some("unconfirmed") => targets::Outcome::Unconfirmed,
                Some("skipped" | "unreachable" | "rejected") => targets::Outcome::Skipped,
                _ => return Err("Unknown Baritone terminal outcome.".into()),
            };
            if outcome == targets::Outcome::Mined {
                let settled = game.sunset_baritone_observation(pos).is_some_and(|(id, pending)| pending.is_none()
                    && !targets::matches_kind(kind, &game.res.facts().get(horizon_world::registry::BlockStateId(id)).name));
                if !settled || !job.verified.contains(&pos) {
                    return Err("A mined target was not verified and settled by the native prediction handler.".into());
                }
            }
            if ledger.finish(key, pos, outcome) { job.offered.remove(&pos); job.revision = 0; }
        }
    }
    let counts = if let Some(ledger) = &job.ledger {
        let c = ledger.counts(); [c.mined as u64, c.skipped as u64, c.mismatch as u64, c.unconfirmed as u64]
    } else { ["mined", "skipped", "mismatch", "unconfirmed"].map(|k| value["counters"][k].as_u64().unwrap_or(0)) };
    let mut status: String = value["status"].as_str().unwrap_or("Idle").chars().take(80).collect();
    if value["active"].as_bool() == Some(false) && let Some(detail) = value["detail"].as_str().filter(|s| !s.is_empty()) {
        status.push_str(": "); status.extend(detail.chars().take(160));
    }
    let draw = &value["draw"];
    Ok((status, counts, DrawSnapshot { world: job.world, dimension: job.dimension, job: job.generation,
        path: positions(&draw["path"], 2048), next_path: positions(&draw["next_path"], 2048),
        goal: position(&draw["goal"]), target: position(&draw["target"]),
        breaking: positions(&draw["breaking"], 4096), placing: positions(&draw["placing"], 4096) }))
}

pub(super) fn pre_tick(game: &mut Game<'_>, mods: &[Module]) {
    maintain(game);
    if !game.native_window() { return; }
    let Some(mut job) = state(|c| c.job.take()) else { return; };
    if job.last_tick == Some(game.ticks) { state(|c| c.job = Some(job)); return; }
    job.last_tick = Some(game.ticks);
    let result = (|| {
        let service = ready(game).ok_or("Baritone companion became unavailable.")?;
        let block_key = (game.world.revision, game.world.chunks.membership_revision());
        if job.block_key != block_key {
            job.block_key = block_key;
            job.block_revision = job.block_revision.wrapping_add(1).max(1);
        }
        let current = mods.iter().find(|m| m.name == NAME).ok_or("Missing Baritone module.")?;
        // Execution edits cancel in maintain(); rendering preferences are read live by the Sunset draw adapter.
        if !current.on || owner::conflict(mods).is_some() { return Err("Baritone lost its module/input owner.".into()); }
        if matches!(job.source, Source::Predicted) {
            refill(game, &mut job, service, false)?;
            if job.ledger.is_none() {
                owner::keys(game, job.lease, &[]);
                state(|c| c.status = "Waiting for seed/replay".into());
                return Ok(true);
            }
        }
        let value = invoke(service, game, 5, &frame(game, &job)?)?;
        if !response_matches(&value, job.world, job.dimension, job.generation, job.lease.generation, game.ticks)
            || state(|c| c.generation) != job.generation || owner::current() != Some(job.lease) {
            return Err("Discarded stale Baritone output.".into());
        }
        let intent: actions::Intent = serde_json::from_value(value.clone()).map_err(|e| format!("Invalid Baritone actions: {e}"))?;
        if intent.movement & 64 != 0 && !job.settings.b("Sprint") { return Err("Baritone requested disabled sprinting.".into()); }
        if let Some(attack) = intent.attack.as_ref() {
            if let Some(&kind) = job.offered.get(&attack.pos) {
                if !targets::matches_kind(kind, &game.res.facts().get(horizon_world::registry::BlockStateId(attack.state_id)).name) {
                    return Err("A predicted target cannot be broken before its exact ore family is revealed.".into());
                }
            }
        }
        actions::apply(game, job.lease, &intent, job.settings.b("Allow break"), job.settings.b("Allow place"))?;
        job.selected = game.selected;
        if let Some(attack) = intent.attack.as_ref().filter(|a| job.offered.contains_key(&a.pos)) { job.verified.insert(attack.pos); }
        let (status, counts, draw) = update(game, &mut job, &value)?;
        state(|c| { c.status = status; c.counts = counts; c.draw = Some(draw); });
        Ok::<bool, String>(value["active"].as_bool().unwrap_or(false))
    })();
    match result {
        Ok(true) => { state(|c| { if c.generation == job.generation { c.job = Some(job); } }); }
        Ok(false) => {
            actions::release(game, job.lease); owner::release(game, job.lease);
            if state(|c| c.generation) == job.generation {
                if let Some(service) = api::baritone_service() { let _ = catch_unwind(AssertUnwindSafe(service.cancel)); }
                state(|c| c.draw = None);
            }
        }
        Err(error) => {
            actions::release(game, job.lease); owner::release(game, job.lease);
            if state(|c| c.generation) != job.generation { return; }
            stop(game, StopReason::EngineError);
            state(|c| c.status = format!("Stopped: {}", error.chars().take(160).collect::<String>()));
            super::sunset::say(game, &format!("Baritone stopped: {error}"));
        }
    }
}

pub(super) fn post_tick(game: &mut Game<'_>) { actions::post_tick(game); maintain(game); }
pub(super) fn info(_module: &Module) -> Option<String> {
    Some(state(|c| if c.counts.iter().any(|&n| n != 0) {
        format!("{} {}/{}", c.status, c.counts[0], c.counts[1].saturating_add(c.counts[2]).saturating_add(c.counts[3]))
    } else { c.status.clone() }))
}
pub(super) fn draw_snapshot() -> Option<DrawSnapshot> { state(|c| c.draw.clone()) }

fn status() -> String {
    state(|c| {
        let source = c.job.as_ref().map(|j| match j.source { Source::Goto(_) => "goto", Source::Visible(_) => "Visible ores", Source::Predicted => "Ore Sim" }).unwrap_or("none");
        let target = c.draw.as_ref().and_then(|d| d.target).map(|p| format!("{} {} {}", p[0], p[1], p[2])).unwrap_or_else(|| "none".into());
        format!("Baritone: {}; source={source}; target={target}; mined={} skipped={} mismatch={} unconfirmed={}.",
            c.status, c.counts[0], c.counts[1], c.counts[2], c.counts[3])
    })
}
fn diagnostic(game: &Game<'_>, limit: usize) -> Result<String, String> {
    let module = super::sunset::with(|s| s.module(NAME).cloned()).ok_or("Missing Baritone settings.")?;
    let ordinary = state(|c| c.job.as_ref().and_then(|j| {
        let (source, pos) = match &j.source {
            Source::Predicted => return None,
            Source::Goto(goal) => ("goto", Some(*goal)),
            Source::Visible(_) => ("Visible ores", c.draw.as_ref().and_then(|d| d.target)),
        };
        Some((source, j.world, j.dimension, pos))
    }));
    if let Some((source, world, dimension, pos)) = ordinary {
        let mut text = format!("Baritone targets: source={source} world={world} dimension={dimension};");
        if let Some(pos) = pos {
            let name: Option<&str> = game.sunset_baritone_observation(pos).map(|(id, _)| game.res.facts().get(horizon_world::registry::BlockStateId(id)).name.as_ref());
            text.push_str(&format!(" {} {} {} block={} state={};", pos[0], pos[1], pos[2], name.unwrap_or("unknown"),
                if name.is_some() { "observed" } else { "unknown" }));
        }
        return Ok(text);
    }
    let (query, excluded) = state(|c| c.job.as_ref().map(|j| (ores::MiningQuery { anchor: j.query.anchor,
        kind_mask: j.query.kind_mask, min_y: j.query.min_y, max_y: j.query.max_y, radius: j.query.radius, limit: j.query.limit },
        j.ledger.as_ref().map_or_else(HashSet::new, |l| l.exclusions().clone()))))
        .map(Ok).unwrap_or_else(|| query(game, &module, targets::parse_kinds(module.t("Ores"))?).map(|q| (q, HashSet::new())))?;
    let snapshot = ores::mining_snapshot(game, &query, &excluded).map_err(|why| format!("Ore Sim targets unavailable: {why:?}."))?;
    let selected = targets::ordered_candidates(&snapshot, &query, feet(game).ok_or("No player.")?, &excluded, limit);
    let mut text = format!("Baritone targets: world={} replay={} dimension={} revision={} pending={} completed_chunks={};",
        snapshot.key.world, snapshot.key.replay, snapshot.key.dimension, snapshot.revision, snapshot.pending, snapshot.completed_chunks);
    for ore in selected.into_iter().take(limit) {
        let state = game.sunset_baritone_observation(ore.pos).map(|(id, _)| {
            let name = &game.res.facts().get(horizon_world::registry::BlockStateId(id)).name;
            if targets::matches_kind(ore.kind, name) { "verified" } else if targets::plausible_state(ore.kind, name) { "predicted" } else { "mismatch" }
        }).unwrap_or("unknown");
        text.push_str(&format!(" {} {} {} kind={} state={state};", ore.pos[0], ore.pos[1], ore.pos[2], targets::kind_name(ore.kind).unwrap_or("unknown")));
    }
    Ok(text)
}

pub(super) fn command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    let command = match parse(args) {
        Ok(command) => command,
        Err(error) => {
            let Some(full) = api::baritone_service().and_then(|s| s.full) else { return Err(error); };
            (full.command)(game, &args.join(" "))?;
            return Ok(String::new());
        }
    };
    match command {
        Command::Status => {
            maintain(game);
            if let Some(text) = api::baritone_service().and_then(|s| s.full).and_then(|f| (f.status)()) { return Ok(text); }
            Ok(status())
        }
        Command::Help => Ok(".b / .baritone status | goto <x> <y> <z> | mine [block[,block...]] | mine oresim [ore[,ore...]] | stop | off | targets [1..64] | help. Bare mine uses the Source/Ores settings. The complete original runtime is available through #help, including build, farm, follow, explore, elytra, selections, waypoints and settings; its other commands also accept .b. Ore Sim must already be on. Source notices and engine replacement: docs/baritone.md.".into()),
        Command::Stop => { stop(game, StopReason::User); Ok("Baritone stopped; controls released.".into()) }
        Command::Off => {
            stop(game, StopReason::Disabled);
            super::sunset::with(|s| { if let Some(m) = s.modules.iter_mut().find(|m| m.name == NAME) { m.on = false; } });
            super::sunset::save(game); Ok("Baritone off; controls released.".into())
        }
        Command::Targets(limit) => diagnostic(game, limit),
        start_command => start(game, start_command),
    }
}
