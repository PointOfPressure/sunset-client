// Independent MIT Horizon adapter. The DLL supplies all path/mining policy;
// these queries only report the host's actual read-only observations.
use super::{Game, sunset_api as api};
use serde_json::{Value, json};
use std::cell::RefCell;
use sunset_baritone_loader::{Host, Runtime, abi::BlockReadV1};

const ID: &str = "sunset_baritone";
const _: () = api::needs(3);

// A sliced search depends on blocks it actually inspected, not every changing
// snow layer or fluid in the loaded world. Keep the complete observation (also
// the server/prediction state), and compare in native code before lending the
// engine a stable terrain revision. This does not rely on block-event coverage.
mod scoped_revisions {
    use std::collections::{HashMap, HashSet};
    pub(super) type Fingerprint = [u32; 5];
    const MAX_OBSERVATIONS: usize = 1 << 20;

    pub(super) struct Scope {
        identity: Option<String>, revisions: Option<(u64, u64)>, epoch: u64,
        observations: HashMap<[i32; 3], Fingerprint>,
        query_regions: HashSet<([i32; 3], i32)>,
        overflow: bool, limit: usize,
    }
    impl Default for Scope {
        fn default() -> Self { Self::new(MAX_OBSERVATIONS) }
    }
    impl Scope {
        fn new(limit: usize) -> Self {
            Self { identity: None, revisions: None, epoch: 0, observations: HashMap::new(),
                query_regions: HashSet::new(), overflow: false, limit }
        }
        pub(super) fn clear(&mut self) {
            self.identity = None; self.revisions = None;
            self.observations.clear(); self.query_regions.clear(); self.overflow = false;
        }
        pub(super) fn begin(&mut self, identity: String, revisions: (u64, u64),
            mut read: impl FnMut([i32; 3]) -> Fingerprint) -> u64 {
            let reset = self.identity.as_ref() != Some(&identity)
                || self.revisions.is_some_and(|old| old.1 != revisions.1);
            if reset {
                self.observations.clear(); self.query_regions.clear(); self.overflow = false;
            }
            let global_changed = self.revisions != Some(revisions);
            let mut changed = reset || self.overflow && global_changed;
            for (&pos, previous) in &mut self.observations {
                // Confirmations can clear pending metadata without changing the
                // effective block state or the world's block revision.
                if global_changed || previous[3] != 0 {
                    let current = read(pos);
                    changed |= *previous != current;
                    *previous = current;
                }
            }
            self.identity = Some(identity); self.revisions = Some(revisions);
            if changed { self.epoch = self.epoch.wrapping_add(1).max(1); }
            self.epoch
        }
        pub(super) fn record(&mut self, pos: [i32; 3], value: Fingerprint) {
            if self.observations.contains_key(&pos) || self.overflow { return; }
            if self.observations.len() >= self.limit {
                self.overflow = true;
                eprintln!("[Sunset Baritone] dependency tracking reached {} blocks; using global terrain revisions", self.limit);
            } else { self.observations.insert(pos, value); }
        }
        pub(super) fn region(&mut self, lo: [i32; 3], hi: [i32; 3],
            mut read: impl FnMut([i32; 3]) -> Fingerprint) {
            if self.overflow { return; }
            for y in lo[1]..=hi[1] { for z in lo[2]..=hi[2] { for x in lo[0]..=hi[0] {
                let p = [x, y, z];
                if !self.observations.contains_key(&p) { self.record(p, read(p)); }
                if self.overflow { return; }
            } } }
        }
        pub(super) fn query_region(&mut self, center: [i32; 3], radius: i32) -> bool {
            !self.overflow && self.query_regions.insert((center, radius))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn block(state: u32) -> Fingerprint { [state, state, 1, 0, 0] }
        #[test]
        fn baritone_scoped_revision_ignores_unrelated_writes_but_detects_unreported_dependency_changes() {
            let p = [7, -40, 8]; let mut scope = Scope::new(32);
            let first = scope.begin("job".into(), (1, 2), |_| block(1));
            scope.record(p, block(1));
            assert_eq!(scope.begin("job".into(), (2, 2), |_| block(1)), first);
            assert_eq!(scope.begin("job".into(), (3, 2), |_| block(2)), first + 1);
            assert!(scope.observations.contains_key(&p), "executor dependencies survive invalidation");
        }
        #[test]
        fn baritone_scoped_revision_pending_confirmation_does_not_need_a_block_write() {
            let mut scope = Scope::new(32);
            let first = scope.begin("job".into(), (1, 2), |_| block(0));
            scope.record([0, 0, 0], [0, 7, 1, 1, 42]);
            assert_eq!(scope.begin("job".into(), (1, 2), |_| block(0)), first + 1);
        }
        #[test]
        fn baritone_scoped_revision_world_membership_and_overflow_are_conservative() {
            let mut scope = Scope::new(1);
            let first = scope.begin("job".into(), (1, 2), |_| block(1));
            scope.record([0, 0, 0], block(1)); scope.record([1, 0, 0], block(1));
            assert!(scope.overflow);
            assert_eq!(scope.begin("job".into(), (2, 2), |_| block(1)), first + 1);
            let next = scope.begin("job".into(), (2, 3), |_| block(1));
            assert_eq!(next, first + 2); assert!(scope.observations.is_empty()); assert!(!scope.overflow);
            assert_eq!(scope.begin("other job".into(), (2, 3), |_| block(1)), next + 1);
        }
        #[test]
        fn baritone_scoped_revision_tracks_neighbors_and_native_query_regions_once() {
            let mut scope = Scope::new(128);
            let first = scope.begin("job".into(), (1, 2), |_| block(1));
            scope.region([-1, -1, -1], [1, 1, 1], |_| block(1));
            assert_eq!(scope.observations.len(), 27);
            assert!(scope.query_region([0, 0, 0], 1)); assert!(!scope.query_region([0, 0, 0], 1));
            assert_eq!(scope.begin("job".into(), (2, 2), |p| block(if p == [1, 1, 1] { 2 } else { 1 })), first + 1);
        }
    }
}

#[derive(Default)]
struct Loaded {
    runtime: Option<Runtime>,
    attempted: bool,
    error: Option<String>,
    reported: bool,
    dependencies: RefCell<scoped_revisions::Scope>,
}
thread_local! {static ENGINE:RefCell<Loaded>=RefCell::new(Loaded::default());}

fn enabled(game: &Game<'_>) -> bool {
    game.native_window() && game.native_on(ID) && game.native_on("sunset") && api::running()
}

pub(super) fn tick(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    if !enabled(game) {
        cancel();
        return;
    }
    api::plugin(game, ID, "1.0.0", |plugin| {
        let registered = api::register_baritone(api::BaritoneServiceV1 {
            version: 1,
            call,
            available,
            cancel,
            full: Some(api::BaritoneFullV1 {
                command: super::sunset_baritone_full::dispatch,
                stop: super::sunset_baritone_full::stop,
                status: super::sunset_baritone_full::status,
            }),
        });
        if let Err(error) = registered {
            eprintln!("Sunset Baritone registration: {error}");
        }
        plugin
            .on_stop(|game, _| { super::sunset_baritone_full::stop(game); cancel(); })
            .on_disconnect(|game, _| { super::sunset_baritone_full::stop(game); cancel(); })
            .on_death(|game| { super::sunset_baritone_full::stop(game); cancel(); });
    });
    let error = ENGINE.with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else {
            return None;
        };
        if !state.attempted {
            state.attempted = true;
            let result = sunset_baritone_loader::engine_path(&game.native_data_dir(ID), sunset_baritone_loader::bundled_engine_bytes())
                .and_then(|path| Runtime::open(&path));
            match result {
                Ok(runtime) => state.runtime = Some(runtime),
                Err(error) => state.error = Some(error),
            }
        }
        if !state.reported {
            state.reported = true;
            state.error.clone()
        } else {
            None
        }
    });
    if let Some(error) = error {
        game.native_chat(&format!("Sunset Baritone Unavailable: {error}"));
    }
}

fn available(game: &Game<'_>) -> bool {
    enabled(game)
        && ENGINE.with(|cell| cell.try_borrow().is_ok_and(|state| state.runtime.is_some()))
}
fn call(game: &Game<'_>, operation: u32, request: &[u8]) -> Result<Vec<u8>, String> {
    if !enabled(game) {
        return Err("Sunset Baritone companion is disabled".into());
    }
    ENGINE.with(|cell| {
        let mut state = cell
            .try_borrow_mut()
            .map_err(|_| "Baritone engine call is reentrant")?;
        let rewritten;
        let request = if operation == 5 {
            let mut frame: Value = serde_json::from_slice(request).map_err(|e| e.to_string())?;
            let identity = json!([frame["world"], frame["dimension"], frame["job_generation"],
                frame["lease_generation"], frame["registry_epoch"], frame["settings_revision"],
                game.world.shape.min_y, game.world.shape.max_y(), game.border.bounds()]).to_string();
            let epoch = state.dependencies.borrow_mut().begin(identity,
                (game.world.revision, game.world.chunks.membership_revision()), |p| fingerprint(read_block(game, p)));
            frame["block_revision"] = json!(epoch.to_string());
            rewritten = serde_json::to_vec(&frame).map_err(|e| e.to_string())?;
            rewritten.as_slice()
        } else { request };
        let Loaded { runtime, dependencies, .. } = &mut *state;
        let runtime = runtime
            .as_mut()
            .ok_or("Baritone engine is unavailable")?;
        runtime.call(operation, request, Some(&Reads { game, dependencies }))
    })
}
fn cancel() {
    ENGINE.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            state.dependencies.borrow_mut().clear();
            if let Some(runtime) = state.runtime.as_mut() {
                let _ = runtime.cancel();
            }
        }
    });
}

fn read_block(game: &Game<'_>, pos: [i32; 3]) -> BlockReadV1 {
    match game.sunset_baritone_observation(pos) {
        None => BlockReadV1::default(),
        Some((state, pending)) => BlockReadV1 { state_id: state,
            server_state_id: pending.map_or(state, |(_, s)| s), loaded: 1,
            prediction_pending: u32::from(pending.is_some()), prediction_sequence: pending.map_or(0, |(seq, _)| seq) },
    }
}
fn fingerprint(value: BlockReadV1) -> scoped_revisions::Fingerprint {
    [value.state_id, value.server_state_id, value.loaded, value.prediction_pending, value.prediction_sequence as u32]
}
struct Reads<'a, 'b> { game: &'a Game<'b>, dependencies: &'a RefCell<scoped_revisions::Scope> }
impl Reads<'_, '_> {
    fn track_neighborhood(&self, pos: [i32; 3]) {
        self.dependencies.borrow_mut().region(pos.map(|n| n.saturating_sub(1)), pos.map(|n| n.saturating_add(1)),
            |p| fingerprint(read_block(self.game, p)));
    }
    fn track_ray_region(&self, eye: [f64; 3], reach: f64) {
        let center = eye.map(|n| (n.floor() as i32).clamp(-30_000_000, 30_000_000));
        let radius = reach.ceil() as i32 + 2;
        let mut dependencies = self.dependencies.borrow_mut();
        if dependencies.query_region(center, radius) {
            dependencies.region(center.map(|n| n - radius), center.map(|n| n + radius),
                |p| fingerprint(read_block(self.game, p)));
        }
    }
}
fn integer(v: &Value) -> Result<i32, String> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| "invalid integer".into())
}
fn number(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| "invalid number".into())
}
fn pos(v: &Value) -> Result<[i32; 3], String> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or("invalid block position")?;
    let p = [integer(&a[0])?, integer(&a[1])?, integer(&a[2])?];
    if p.iter().any(|n| n.abs() > 30_000_000) {
        return Err("block position outside world".into());
    }
    Ok(p)
}
fn vector(v: &Value) -> Result<[f64; 3], String> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or("invalid vector")?;
    Ok([number(&a[0])?, number(&a[1])?, number(&a[2])?])
}
fn view(yaw: f64, pitch: f64) -> [f64; 3] {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}
fn aim(eye: [f64; 3], point: [f64; 3]) -> [f64; 2] {
    let d = std::array::from_fn::<_, 3, _>(|i| point[i] - eye[i]);
    [
        (-d[0]).atan2(d[2]).to_degrees(),
        (-d[1]).atan2(d[0].hypot(d[2])).to_degrees(),
    ]
}

impl Host for Reads<'_, '_> {
    fn block(&self, pos: [i32; 3]) -> Result<BlockReadV1, String> {
        let value = read_block(self.game, pos);
        self.dependencies.borrow_mut().record(pos, fingerprint(value));
        self.track_neighborhood(pos);
        Ok(value)
    }
    fn query(&self, operation: u32, bytes: &[u8]) -> Result<Vec<u8>, String> {
        let v: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if v["schema"].as_u64() != Some(1) {
            return Err("invalid host query schema".into());
        }
        let game = self.game;
        let result = match operation {
            1 => {
                let id = v["state_id"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or("invalid state id")?;
                let state = horizon_world::registry::BlockStateId(id);
                let descriptor = game
                    .pack
                    .assets
                    .registry
                    .state(state)
                    .ok_or("unknown state id")?;
                let f = game.res.facts().get(state);
                let fluid = match f.fluid {
                    horizon_client::blocks::Fluid::None => "none",
                    horizon_client::blocks::Fluid::Water
                        if f.fluid_level == 8 && !f.fluid_falling =>
                    {
                        "water"
                    }
                    horizon_client::blocks::Fluid::Water => "flowing_water",
                    horizon_client::blocks::Fluid::Lava
                        if f.fluid_level == 8 && !f.fluid_falling =>
                    {
                        "lava"
                    }
                    horizon_client::blocks::Fluid::Lava => "flowing_lava",
                };
                let mut bounds: Option<[f64; 6]> = None;
                for b in f.outline.iter() {
                    bounds = Some(match bounds {
                        None => *b,
                        Some(mut full) => {
                            for i in 0..3 {
                                full[i] = full[i].min(b[i]);
                                full[i + 3] = full[i + 3].max(b[i + 3]);
                            }
                            full
                        }
                    });
                }
                json!({"schema":1,"name":f.name.as_ref(),"classes":f.classes.iter().map(|c|c.as_ref()).collect::<Vec<_>>(),
                    "properties":descriptor.properties,"fluid":fluid,"fluid_amount":f.fluid_level,"hardness":f.hardness,
                    "replaceable":f.replaceable,"full_collision":f.full_collision,"sturdy_top":f.sturdy_full&2!=0,
                    "pathfindable":game.res.mob_data().state(state).flags & horizon_client::mobs::data::LAND!=0,"bounds":bounds})
            }
            2 => {
                if let Some(eye) = game.eye_position() { self.track_neighborhood(eye.map(|n| n.floor() as i32)); }
                let id = v["state_id"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or("invalid state id")?;
                let slot = v["slot"]
                    .as_u64()
                    .filter(|n| *n < 9)
                    .ok_or("invalid hotbar slot")? as usize;
                let (strength, harvestable) = game
                    .sunset_baritone_tool_strength(id, slot)
                    .ok_or("tool strength unavailable")?;
                json!({"schema":1,"strength":strength,"harvestable":harvestable})
            }
            3 => {
                let eye = vector(&v["eye"])?;
                let yaw = number(&v["yaw"])?;
                let pitch = number(&v["pitch"])?;
                let reach = number(&v["reach"])?.clamp(0.0, game.block_reach);
                self.track_ray_region(eye, reach);
                if let Some(hit) = game.clip_ray(eye, view(yaw, pitch), reach) {
                    json!({"schema":1,"pos":hit.pos,"face":hit.face})
                } else {
                    json!({"schema":1,"pos":null})
                }
            }
            4 => {
                let p = pos(&v["pos"])?;
                self.track_neighborhood(p);
                let mut eye = game.eye_position().ok_or("player eyes unavailable")?;
                if v["sneak"].as_bool() == Some(true) {
                    eye[1] -= 0.35;
                }
                let reach = number(&v["reach"])?.clamp(0.0, game.block_reach);
                self.track_ray_region(eye, reach);
                let state = game.world.block(p[0], p[1], p[2]);
                let f = game.res.facts().get(state);
                let mut points = vec![p.map(|n| f64::from(n) + 0.5)];
                let bounds = game
                    .context_outline(p, state)
                    .map(|b| vec![b])
                    .unwrap_or_else(|| f.outline.to_vec());
                for b in bounds.iter().take(32) {
                    let center = std::array::from_fn::<_, 3, _>(|i| (b[i] + b[i + 3]) * 0.5);
                    for axis in 0..3 {
                        for side in 0..2 {
                            let mut at = center;
                            at[axis] = if side == 0 {
                                b[axis] + 1.0e-4
                            } else {
                                b[axis + 3] - 1.0e-4
                            };
                            points.push(std::array::from_fn(|i| at[i] + f64::from(p[i])));
                        }
                    }
                }
                let rotation = points.into_iter().find_map(|point| {
                    let r = aim(eye, point);
                    game.clip_ray(eye, view(r[0], r[1]), reach)
                        .filter(|hit| hit.pos == p)
                        .map(|_| r)
                });
                json!({"schema":1,"yaw":rotation.map(|r|r[0]),"pitch":rotation.map(|r|r[1])})
            }
            5 => return Err("unsupported".into()),
            6 => json!({"schema":1,"mobs":game.mobs.iter().filter(|m|m.alive()).take(256)
                .map(|m|[m.position.x.floor() as i32,m.position.y.floor() as i32,m.position.z.floor() as i32]).collect::<Vec<_>>(),
                "spawners":[],"falling":[],"items":[]}),
            7 => {
                let b = game.border.bounds();
                let mut chunks: Vec<_> =
                    game.world.chunks.iter().map(|(p, _)| [p.0, p.1]).collect();
                chunks.sort_unstable();
                chunks.truncate(16384);
                json!({"schema":1,"min_y":game.world.shape.min_y,"max_y":game.world.shape.max_y()-1,
                    "border_min_x":b[0].ceil() as i32,"border_min_z":b[1].ceil() as i32,
                    "border_max_x":b[2].floor() as i32-1,"border_max_z":b[3].floor() as i32-1,"loaded_chunks":chunks})
            }
            _ => return Err("unsupported".into()),
        };
        serde_json::to_vec(&result).map_err(|e| e.to_string())
    }
}
