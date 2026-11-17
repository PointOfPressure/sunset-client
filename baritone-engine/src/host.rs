use crate::abi::{BlockReadV1, HOST_UNAVAILABLE, UNSUPPORTED};
use crate::baritone_block_state::{BlockState, Class, FluidKind};
use crate::baritone_block_state_interface::BlockStateInterface;
use crate::baritone_calculation_context::{PlayerFacts, ToolSet};
use crate::baritone_movement_state::Input;
use crate::baritone_player_context::PlayerContext;
use crate::baritone_rotation::Rotation;
use crate::baritone_utils::Pos;
use crate::protocol::{self, Frame};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

/// Synchronous, read-only host calls. The FFI adapter is the only implementation
/// that touches raw pointers. Tests implement this directly using paper worlds.
pub trait Host {
    fn block(&self, pos: Pos) -> protocol::Result<BlockReadV1>;
    fn query(&self, operation: u32, request: &Value) -> protocol::Result<Value>;
}

pub struct Facts {
    pub state: Rc<BlockState>,
    pub bounds: Option<[f64; 6]>,
}

#[derive(Clone, Copy, Debug)]
pub struct Tool {
    pub slot: usize,
    pub strength: f64,
    pub harvestable: bool,
}

#[derive(Default)]
pub struct Intents {
    pub inputs: HashSet<Input>,
    pub look: Option<Rotation>,
    pub slot: Option<usize>,
}

pub struct HostWorld<'a> {
    pub host: &'a dyn Host,
    pub frame: &'a Frame,
    pub facts: &'a RefCell<HashMap<u32, Facts>>,
    pub intents: RefCell<Intents>,
    previous_look: Option<Rotation>,
    blocks: RefCell<HashMap<Pos, BlockReadV1>>,
    tools: RefCell<HashMap<(u32, bool), Option<Tool>>>,
    failure: RefCell<Option<i32>>,
    unknown: Rc<BlockState>,
    range: Value,
    pub anchor: Option<([f64; 3], f64)>,
}

impl<'a> HostWorld<'a> {
    pub fn new(
        host: &'a dyn Host,
        frame: &'a Frame,
        facts: &'a RefCell<HashMap<u32, Facts>>,
        anchor: Option<([f64; 3], f64)>,
    ) -> protocol::Result<Self> {
        let range = host.query(7, &json!({"schema":1}))?;
        let min = range["min_y"].as_i64().ok_or(HOST_UNAVAILABLE)?;
        let max = range["max_y"].as_i64().ok_or(HOST_UNAVAILABLE)?;
        if min != i64::from(frame.min_y) || max != i64::from(frame.max_y) {
            return Err(HOST_UNAVAILABLE);
        }
        let unknown = Rc::new(BlockState {
            id: u32::MAX,
            name: Arc::from("sunset:unknown"),
            classes: 0,
            props: Arc::from([]),
            fluid: FluidKind::None,
            fluid_amount: 0,
            hardness: -1.0,
            replaceable: false,
            full_collision: true,
            sturdy_top: false,
            pathfindable: false,
        });
        Ok(Self {
            host,
            frame,
            facts,
            intents: RefCell::new(Intents::default()),
            previous_look: None,
            blocks: RefCell::new(HashMap::new()),
            tools: RefCell::new(HashMap::new()),
            failure: RefCell::new(None),
            unknown,
            range,
            anchor,
        })
    }
    pub fn with_previous_look(mut self, look: Option<Rotation>) -> Self {
        self.previous_look = look;
        self
    }
    fn fail(&self, code: i32) {
        self.failure.borrow_mut().get_or_insert(code);
    }
    pub fn check(&self) -> protocol::Result<()> {
        self.failure.borrow().map_or(Ok(()), Err)
    }
    pub fn read(&self, pos: Pos) -> BlockReadV1 {
        if let Some(value) = self.blocks.borrow().get(&pos) {
            return *value;
        }
        let value = match self.host.block(pos) {
            Ok(value) if value.loaded <= 1 && value.prediction_pending <= 1 => value,
            _ => {
                self.fail(HOST_UNAVAILABLE);
                BlockReadV1::default()
            }
        };
        self.blocks.borrow_mut().insert(pos, value);
        value
    }
    pub fn state(&self, id: u32) -> Rc<BlockState> {
        if let Some(facts) = self.facts.borrow().get(&id) {
            return facts.state.clone();
        }
        match self.read_facts(id) {
            Ok(facts) => {
                let state = facts.state.clone();
                self.facts.borrow_mut().insert(id, facts);
                state
            }
            Err(code) => {
                self.fail(code);
                self.unknown.clone()
            }
        }
    }
    fn read_facts(&self, id: u32) -> protocol::Result<Facts> {
        let v = self.host.query(1, &json!({"schema":1,"state_id":id}))?;
        let name = v["name"]
            .as_str()
            .filter(|s| s.len() <= 256)
            .ok_or(HOST_UNAVAILABLE)?;
        let hardness = protocol::number(&v["hardness"])?;
        if !(-1.0..=1.0e7).contains(&hardness) {
            return Err(HOST_UNAVAILABLE);
        }
        let classes = Class::flags_of(
            v["classes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        );
        let props: Vec<(Arc<str>, Arc<str>)> = v["properties"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| v.as_str().map(|s| (Arc::from(k.as_str()), Arc::from(s))))
            .collect();
        let fluid = match v["fluid"].as_str().unwrap_or("none") {
            "none" => FluidKind::None,
            "water" => FluidKind::Water,
            "flowing_water" => FluidKind::FlowingWater,
            "lava" => FluidKind::Lava,
            "flowing_lava" => FluidKind::FlowingLava,
            _ => return Err(HOST_UNAVAILABLE),
        };
        let mut bounds = None;
        if let Some(array) = v["bounds"].as_array() {
            if array.len() != 6 {
                return Err(HOST_UNAVAILABLE);
            }
            let mut b = [0.0; 6];
            for i in 0..6 {
                b[i] = protocol::number(&array[i])?;
            }
            if (0..3).any(|i| b[i] > b[i + 3]) {
                return Err(HOST_UNAVAILABLE);
            }
            bounds = Some(b);
        }
        Ok(Facts {
            state: Rc::new(BlockState {
                id,
                name: Arc::from(name),
                classes,
                props: props.into(),
                fluid,
                fluid_amount: v["fluid_amount"].as_u64().unwrap_or(0).min(8) as u8,
                hardness: hardness as f32,
                replaceable: v["replaceable"].as_bool().ok_or(HOST_UNAVAILABLE)?,
                full_collision: v["full_collision"].as_bool().ok_or(HOST_UNAVAILABLE)?,
                sturdy_top: v["sturdy_top"].as_bool().ok_or(HOST_UNAVAILABLE)?,
                pathfindable: v["pathfindable"].as_bool().ok_or(HOST_UNAVAILABLE)?,
            }),
            bounds,
        })
    }
    pub fn best_tool(&self, id: u32, harvest: bool) -> Option<Tool> {
        if let Some(tool) = self.tools.borrow().get(&(id, harvest)) {
            return *tool;
        }
        let mut best: Option<Tool> = None;
        for slot in 0..9 {
            let Ok(v) = self
                .host
                .query(2, &json!({"schema":1,"state_id":id,"slot":slot}))
            else {
                continue;
            };
            let Some(strength) = v["strength"]
                .as_f64()
                .filter(|s| s.is_finite() && *s > 0.0 && *s <= 1.0e9)
            else {
                continue;
            };
            let harvestable = v["harvestable"].as_bool().unwrap_or(false);
            if harvest && !harvestable {
                continue;
            }
            if best.is_none_or(|t| strength > t.strength) {
                best = Some(Tool {
                    slot,
                    strength,
                    harvestable,
                });
            }
        }
        self.tools.borrow_mut().insert((id, harvest), best);
        best
    }
    pub fn reachable_rotation(&self, pos: Pos) -> Option<Rotation> {
        self.reachable(pos, self.frame.reach(), false)
    }
    pub fn hit(&self, rotation: Rotation, reach: f64, sneak: bool) -> Option<(Pos, Pos)> {
        let mut eye = self.frame.eye();
        if sneak {
            eye[1] -= 0.35;
        }
        let v = self
            .host
            .query(
                3,
                &json!({"schema":1,"eye":eye,"yaw":rotation.yaw,"pitch":rotation.pitch,
            "reach":reach,"sneak":sneak}),
            )
            .ok()?;
        let pos = protocol::pos(&v["pos"]).ok()?;
        let face = protocol::pos(&v["face"]).ok()?;
        if face.iter().map(|n| n.abs()).sum::<i32>() != 1 {
            return None;
        }
        let nearest = (0..3)
            .map(|i| {
                let lo = f64::from(pos[i]);
                let hi = lo + 1.0;
                let d = if eye[i] < lo {
                    lo - eye[i]
                } else if eye[i] > hi {
                    eye[i] - hi
                } else {
                    0.0
                };
                d * d
            })
            .sum::<f64>();
        (nearest <= reach * reach + 1.0e-6).then_some((pos, face))
    }
    pub fn contains(&self, pos: Pos) -> bool {
        pos[1] >= self.min_y()
            && pos[1] <= self.max_y()
            && self.entirely_contains(pos[0], pos[2])
            && self.anchor.is_none_or(|(anchor, radius)| {
                (0..3)
                    .map(|i| (f64::from(pos[i]) + 0.5 - anchor[i]).powi(2))
                    .sum::<f64>()
                    <= radius * radius
            })
    }
    pub fn loaded_chunks(&self) -> Vec<[i32; 2]> {
        self.range["loaded_chunks"]
            .as_array()
            .into_iter()
            .flatten()
            .take(16384)
            .filter_map(|v| {
                let a = v.as_array()?;
                if a.len() != 2 {
                    return None;
                }
                Some([
                    protocol::integer(&a[0]).ok()?,
                    protocol::integer(&a[1]).ok()?,
                ])
            })
            .collect()
    }
    fn entities(&self, field: &str) -> Vec<Pos> {
        self.host
            .query(6, &json!({"schema":1}))
            .ok()
            .and_then(|v| v[field].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .take(256)
            .filter_map(|v| protocol::pos(v).ok())
            .collect()
    }
    fn placement(
        &self,
        slot: usize,
        clicked: Pos,
        face: Pos,
        hit: [f64; 3],
        rot: Rotation,
        check: bool,
    ) -> Option<Rc<BlockState>> {
        let v = self
            .host
            .query(
                5,
                &json!({"schema":1,"slot":slot,"clicked":clicked,"face":face,"hit":hit,
            "yaw":rot.yaw,"pitch":rot.pitch,"check":check}),
            )
            .ok()?;
        let id = u32::try_from(v["state_id"].as_u64()?).ok()?;
        Some(self.state(id))
    }
    pub fn resolved_actions(&self, protected: Option<Pos>) -> (u32, Option<Value>, Option<Value>) {
        let intents = self.intents.borrow();
        let down = |i| intents.inputs.contains(&i);
        let mut bits = 0;
        for (i, bit) in [
            (Input::MoveForward, 1),
            (Input::MoveBack, 2),
            (Input::MoveLeft, 4),
            (Input::MoveRight, 8),
            (Input::Jump, 16),
            (Input::Sneak, 32),
            (Input::Sprint, 64),
        ] {
            if down(i) {
                bits |= bit;
            }
        }
        if bits & 3 == 3 {
            bits &= !3;
        }
        if bits & 12 == 12 {
            bits &= !12;
        }
        let hit = if down(Input::ClickLeft) || down(Input::ClickRight) {
            self.hit(
                intents.look.unwrap_or_else(|| self.player_rotations()),
                self.frame.reach(),
                down(Input::Sneak),
            )
        } else {
            None
        };
        let attack = if down(Input::ClickLeft) {
            hit.and_then(|(p, _)| {
                let b = self.read(p);
                let state = self.state(b.state_id);
                (Some(p) != protected
                    && b.loaded != 0
                    && b.prediction_pending == 0
                    && !state.is_air()
                    && !state.has_fluid()
                    && state.hardness >= 0.0
                    && self.best_tool(b.state_id, false).is_some())
                .then(|| json!({"pos":p,"state_id":b.state_id}))
            })
        } else {
            None
        };
        let use_target = if down(Input::ClickRight) {
            hit.and_then(|(p, _)| {
                let b = self.read(p);
                (b.loaded != 0).then(|| json!({"pos":p,"state_id":b.state_id}))
            })
        } else {
            None
        };
        (bits, attack, use_target)
    }
}

impl BlockStateInterface for HostWorld<'_> {
    fn get0(&self, x: i32, y: i32, z: i32) -> Rc<BlockState> {
        let p = [x, y, z];
        if !self.contains(p) {
            return self.unknown.clone();
        }
        let b = self.read(p);
        if b.loaded == 0 {
            self.unknown.clone()
        } else {
            self.state(b.state_id)
        }
    }
    fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.read([x, self.frame.feet()[1].clamp(self.min_y(), self.max_y()), z])
            .loaded
            != 0
    }
    fn min_y(&self) -> i32 {
        self.frame.min_y
    }
    fn max_y(&self) -> i32 {
        self.frame.max_y
    }
    fn entirely_contains(&self, x: i32, z: i32) -> bool {
        let limits = [
            ("border_min_x", x, true),
            ("border_max_x", x, false),
            ("border_min_z", z, true),
            ("border_max_z", z, false),
        ];
        limits.iter().all(|(key, n, min)| {
            self.range[*key].as_i64().is_none_or(|v| {
                if *min {
                    i64::from(*n) >= v
                } else {
                    i64::from(*n) <= v
                }
            })
        }) && x.abs() < 30_000_000
            && z.abs() < 30_000_000
    }
}

impl ToolSet for HostWorld<'_> {
    fn str_vs_block(&self, state: &BlockState) -> f64 {
        self.best_tool(state.id, false).map_or(0.0, |t| t.strength)
    }
}

impl PlayerContext for HostWorld<'_> {
    fn bsi(&self) -> &dyn BlockStateInterface {
        self
    }
    fn player_feet(&self) -> Pos {
        self.frame.feet()
    }
    fn position(&self) -> [f64; 3] {
        self.frame.position
    }
    fn player_head(&self) -> [f64; 3] {
        self.frame.eye()
    }
    fn delta_movement(&self) -> [f64; 3] {
        self.frame.velocity
    }
    fn player_rotations(&self) -> Rotation {
        // Sunset restores the user's camera after applying our tick's aim.
        // Upstream movement gates use the controlled pose from the prior tick;
        // retain that value without retaining a host/world or emitting stale aim.
        // selected_block and final actions still ray trace from the live eye.
        self.intents
            .borrow()
            .look
            .or(self.previous_look)
            .unwrap_or_else(|| Rotation::new(self.frame.yaw, self.frame.pitch))
    }
    fn on_ground(&self) -> bool {
        self.frame.flag("on_ground", true)
    }
    fn is_in_water(&self) -> bool {
        self.frame.flag("in_water", false)
    }
    fn is_swimming(&self) -> bool {
        self.frame.flag("swimming", false)
    }
    fn is_under_water(&self) -> bool {
        self.frame.flag("under_water", false)
    }
    fn is_crouching(&self) -> bool {
        self.frame.flag("crouching", false)
    }
    fn horizontal_collision(&self) -> bool {
        self.frame.flag("horizontal_collision", false)
    }
    fn is_in_wall(&self) -> bool {
        self.frame.flag("in_wall", false)
    }
    fn food_level(&self) -> i32 {
        self.frame.fact("food", 20.0).clamp(0.0, 20.0) as i32
    }
    fn air_supply(&self) -> i32 {
        self.frame.fact("air", 300.0) as i32
    }
    fn max_air_supply(&self) -> i32 {
        self.frame.fact("max_air", 300.0) as i32
    }
    fn water_fluid_height(&self) -> f64 {
        self.frame.fact("water_height", 0.0)
    }
    fn set_sprinting(&self, on: bool) {
        self.set_input_force_state(Input::Sprint, on);
    }
    fn is_nether(&self) -> bool {
        self.frame.identity.dimension == "minecraft:the_nether"
    }
    fn is_sprinting(&self) -> bool {
        self.frame.flag("sprinting", false)
    }
    fn is_in_lava(&self) -> bool {
        self.frame.flag("in_lava", false)
    }
    fn on_climbable(&self) -> bool {
        self.frame.flag("on_climbable", false)
    }
    fn has_unsupported_motion_effects(&self) -> bool {
        self.frame.flag("motion_effects", false)
    }
    fn movement_speed(&self) -> f64 {
        self.frame.fact("movement_speed", 0.13).clamp(0.0, 2.0)
    }
    fn is_looking_at(&self, pos: Pos) -> bool {
        self.selected_block() == Some(pos)
    }
    fn selected_block(&self) -> Option<Pos> {
        self.hit(
            self.player_rotations(),
            self.frame.reach(),
            self.is_crouching(),
        )
        .map(|(p, _)| p)
    }
    fn selected_face(&self) -> Option<Pos> {
        self.hit(
            self.player_rotations(),
            self.frame.reach(),
            self.is_crouching(),
        )
        .map(|(_, f)| f)
    }
    fn block_reach_distance(&self) -> f64 {
        self.frame.reach()
    }
    fn reachable(&self, pos: Pos, reach: f64, sneak: bool) -> Option<Rotation> {
        let v = match self.host.query(
            4,
            &json!({"schema":1,"pos":pos,"reach":reach,"sneak":sneak}),
        ) {
            Ok(v) => v,
            Err(UNSUPPORTED) => return None,
            Err(_) => return None,
        };
        let yaw = v["yaw"]
            .as_f64()
            .filter(|n| n.is_finite() && n.abs() <= 1.0e9)?;
        let pitch = v["pitch"]
            .as_f64()
            .filter(|n| n.is_finite() && n.abs() <= 90.0)?;
        let rot = Rotation::new(yaw as f32, pitch as f32).normalize_and_clamp();
        (self.hit(rot, reach, sneak)?.0 == pos).then_some(rot)
    }
    fn reachable_offset(
        &self,
        pos: Pos,
        _offset: [f64; 3],
        reach: f64,
        sneak: bool,
    ) -> Option<Rotation> {
        self.reachable(pos, reach, sneak)
    }
    fn select_item(&self, select: bool, wanted: &dyn Fn(&str) -> bool) -> bool {
        for item in self.frame.raw["hotbar"]
            .as_array()
            .into_iter()
            .flatten()
            .take(9)
        {
            let slot = item["slot"].as_u64().unwrap_or(9) as usize;
            if slot < 9
                && item["count"].as_u64().unwrap_or(0) > 0
                && item["name"].as_str().is_some_and(wanted)
            {
                if select {
                    self.select_hotbar_slot(slot);
                }
                return true;
            }
        }
        false
    }
    fn ray_trace_towards(&self, rot: Rotation, reach: f64, sneak: bool) -> Option<(Pos, Pos)> {
        self.hit(rot, reach, sneak)
    }
    fn infer_sneaking_eye_position(&self) -> [f64; 3] {
        let mut p = self.frame.eye();
        p[1] -= 0.35;
        p
    }
    fn peek_rotation(&self, rot: Rotation) -> Rotation {
        rot
    }
    fn calculation_facts(&self) -> PlayerFacts {
        PlayerFacts {
            has_throwaway: self.has_generic_throwaway(),
            has_water_bucket: false,
            can_sprint: self.food_level() > 6,
            frost_walker: 0,
            water_speed_multiplier: 0.0,
        }
    }
    fn has_generic_throwaway(&self) -> bool {
        // Placement remains unavailable unless the host explicitly advertises a
        // throwaway stack and implements pure placement feasibility.
        self.frame.raw["hotbar"]
            .as_array()
            .into_iter()
            .flatten()
            .take(9)
            .any(|v| v["throwaway"].as_bool() == Some(true))
    }
    fn select_throwaway_for_location(&self, select: bool, _x: i32, _y: i32, _z: i32) -> bool {
        for item in self.frame.raw["hotbar"]
            .as_array()
            .into_iter()
            .flatten()
            .take(9)
        {
            if item["throwaway"].as_bool() == Some(true) {
                if let Some(slot) = item["slot"].as_u64().filter(|n| *n < 9) {
                    if select {
                        self.select_hotbar_slot(slot as usize);
                    }
                    return true;
                }
            }
        }
        false
    }
    fn switch_to_best_tool_for(&self, pos: Pos) {
        let b = self.read(pos);
        if let Some(tool) = self.best_tool(b.state_id, false) {
            self.select_hotbar_slot(tool.slot);
        }
    }
    fn water_bucket_hotbar_slot(&self) -> Option<usize> {
        None
    }
    fn empty_bucket_hotbar_slot(&self) -> Option<usize> {
        None
    }
    fn select_hotbar_slot(&self, slot: usize) {
        if slot < 9 {
            self.intents.borrow_mut().slot = Some(slot);
        }
    }
    fn state_for_placement(
        &self,
        slot: usize,
        p: Pos,
        f: Pos,
        h: [f64; 3],
        r: Rotation,
    ) -> Option<Rc<BlockState>> {
        self.placement(slot, p, f, h, r, false)
    }
    fn would_place(
        &self,
        slot: usize,
        p: Pos,
        f: Pos,
        h: [f64; 3],
        r: Rotation,
    ) -> Option<Rc<BlockState>> {
        self.placement(slot, p, f, h, r, true)
    }
    fn approx_placeable(&self, size: usize) -> Vec<Rc<BlockState>> {
        vec![self.unknown.clone(); size.min(9)]
    }
    fn shape_bounds(&self, pos: Pos) -> Option<[f64; 6]> {
        let b = self.read(pos);
        self.state(b.state_id);
        self.facts.borrow().get(&b.state_id)?.bounds
    }
    fn can_survive(&self, _pos: Pos, _state: &BlockState) -> bool {
        false
    }
    fn placement_plausible(&self, _pos: Pos, _state: &BlockState) -> bool {
        false
    }
    fn mobs_to_avoid(&self) -> Vec<Pos> {
        self.entities("mobs")
    }
    fn mob_spawners_near(&self) -> Vec<Pos> {
        self.entities("spawners")
    }
    fn falling_block_near(&self, pos: Pos) -> bool {
        self.entities("falling").contains(&pos)
    }
    fn update_look_target(&self, rotation: Rotation, _force: bool) {
        let mut rotation = rotation.normalize_and_clamp();
        if rotation.yaw >= 180.0 {
            rotation.yaw -= 360.0;
        }
        self.intents.borrow_mut().look = Some(rotation);
    }
    fn clear_all_keys(&self) {
        self.intents.borrow_mut().inputs.clear();
    }
    fn set_input_force_state(&self, input: Input, on: bool) {
        if on {
            self.intents.borrow_mut().inputs.insert(input);
        } else {
            self.intents.borrow_mut().inputs.remove(&input);
        }
    }
    fn is_input_forced_down(&self, input: Input) -> bool {
        self.intents.borrow().inputs.contains(&input)
    }
    fn stop_breaking_block(&self) {
        self.set_input_force_state(Input::ClickLeft, false);
    }
}
