use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use sunset_baritone_engine::{
    abi::*,
    engine::Engine,
    host::Host,
    protocol::{self, Frame},
};

const TARGET: [i32; 3] = [1, 64, 0];
struct Paper {
    blocks: RefCell<HashMap<[i32; 3], BlockReadV1>>,
    harvest: Cell<bool>,
    reads: Cell<usize>,
    fail: Cell<bool>,
    real_rays: Cell<bool>,
    eye: Cell<[f64; 3]>,
}
impl Paper {
    fn new() -> Self {
        Self {
            blocks: RefCell::new(HashMap::new()),
            harvest: Cell::new(true),
            reads: Cell::new(0),
            fail: Cell::new(false),
            real_rays: Cell::new(false),
            eye: Cell::new([0.5, 65.62, 0.5]),
        }
    }
    fn set(&self, p: [i32; 3], state: u32, pending: bool, server: u32) {
        self.blocks.borrow_mut().insert(
            p,
            BlockReadV1 {
                state_id: state,
                server_state_id: server,
                loaded: 1,
                prediction_pending: u32::from(pending),
                prediction_sequence: 7,
            },
        );
    }
    fn trace(
        &self,
        eye: [f64; 3],
        yaw: f64,
        pitch: f64,
        reach: f64,
    ) -> protocol::Result<Option<([i32; 3], [i32; 3])>> {
        let (y, p) = (yaw.to_radians(), pitch.to_radians());
        let dir = [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()];
        let mut previous = eye.map(|v| v.floor() as i32);
        for step in 0..=(reach * 128.0).ceil() as usize {
            let distance = (step as f64 / 128.0).min(reach);
            let at = std::array::from_fn(|i| (eye[i] + dir[i] * distance).floor() as i32);
            if self.block(at)?.state_id != 0 {
                let mut face = [0; 3];
                if let Some(axis) = (0..3).find(|i| previous[*i] != at[*i]) {
                    face[axis] = (previous[axis] - at[axis]).signum();
                } else {
                    face[0] = -1;
                }
                return Ok(Some((at, face)));
            }
            previous = at;
        }
        Ok(None)
    }
}
impl Host for Paper {
    fn block(&self, p: [i32; 3]) -> protocol::Result<BlockReadV1> {
        self.reads.set(self.reads.get() + 1);
        if self.fail.get() {
            return Err(HOST_UNAVAILABLE);
        }
        Ok(self
            .blocks
            .borrow()
            .get(&p)
            .copied()
            .unwrap_or(BlockReadV1 {
                state_id: if p[1] < 64 { 1 } else { 0 },
                loaded: 1,
                ..BlockReadV1::default()
            }))
    }
    fn query(&self, op: u32, request: &Value) -> protocol::Result<Value> {
        if self.fail.get() {
            return Err(HOST_UNAVAILABLE);
        }
        Ok(match op {
            1 => {
                let id = request["state_id"].as_u64().unwrap_or(0);
                let (name, hardness) = match id {
                    0 => ("minecraft:air", 0.0),
                    1 => ("minecraft:stone", 1.5),
                    2 => ("minecraft:diamond_ore", 3.0),
                    3 => ("minecraft:bedrock", -1.0),
                    4 => ("minecraft:emerald_ore", 3.0),
                    5 => ("minecraft:lava", 100.0),
                    _ => return Err(HOST_UNAVAILABLE),
                };
                json!({"schema":1,"name":name,"classes":if id==0 {vec!["AirBlock"]} else {vec!["Block"]},
                    "properties":{},"fluid":if id==5 {"lava"} else {"none"},"fluid_amount":if id==5 {8} else {0},"hardness":hardness,"replaceable":id==0,
                    "full_collision":id!=0,"sturdy_top":id!=0,"pathfindable":id==0,"bounds":if id==0 {None} else {Some([0,0,0,1,1,1])}})
            }
            2 => json!({"schema":1,"strength":0.2,"harvestable":self.harvest.get()}),
            3 => {
                if self.real_rays.get() {
                    let hit = self.trace(
                        protocol::vec3(&request["eye"])?,
                        request["yaw"].as_f64().unwrap(),
                        request["pitch"].as_f64().unwrap(),
                        request["reach"].as_f64().unwrap(),
                    )?;
                    return Ok(hit.map_or_else(
                        || json!({"schema":1,"pos":null}),
                        |(p, f)| json!({"schema":1,"pos":p,"face":f}),
                    ));
                }
                let b = self.block(TARGET)?;
                if b.state_id != 0 {
                    json!({"schema":1,"pos":TARGET,"face":[-1,0,0]})
                } else {
                    json!({"schema":1,"pos":null})
                }
            }
            4 => {
                if self.real_rays.get() {
                    let target = protocol::pos(&request["pos"])?;
                    let eye = self.eye.get();
                    let delta =
                        std::array::from_fn::<_, 3, _>(|i| f64::from(target[i]) + 0.5 - eye[i]);
                    let yaw = (-delta[0]).atan2(delta[2]).to_degrees();
                    let pitch = (-delta[1]).atan2(delta[0].hypot(delta[2])).to_degrees();
                    let reaches = self
                        .trace(eye, yaw, pitch, request["reach"].as_f64().unwrap())?
                        .is_some_and(|(p, _)| p == target);
                    return Ok(if reaches {
                        json!({"schema":1,"yaw":yaw,"pitch":pitch})
                    } else {
                        json!({"schema":1,"yaw":null,"pitch":null})
                    });
                }
                if self.block(TARGET)?.state_id != 0 {
                    json!({"schema":1,"yaw":-90,"pitch":0})
                } else {
                    json!({"schema":1,"yaw":null,"pitch":null})
                }
            }
            5 => return Err(UNSUPPORTED),
            6 => json!({"schema":1,"mobs":[],"spawners":[],"falling":[],"items":[]}),
            7 => json!({"schema":1,"min_y":-64,"max_y":319,"loaded_chunks":[[0,0]]}),
            _ => return Err(UNSUPPORTED),
        })
    }
}
fn envelope() -> Value {
    json!({"schema":1,"world":"1","dimension":"minecraft:overworld","job_generation":"2","lease_generation":"3"})
}
fn frame(tick: u64) -> Value {
    let mut f = envelope();
    f["tick"] = json!(tick.to_string());
    for key in [
        "registry_epoch",
        "block_revision",
        "settings_revision",
        "target_revision",
    ] {
        f[key] = json!("1");
    }
    f["player"] = json!({"position":[0.5,64.0,0.5],"velocity":[0,0,0],"yaw":-90,"pitch":0});
    f["input_available"] = json!(true);
    f
}
fn configure(e: &mut Engine, cap: usize) {
    e.dispatch(
        1,
        &json!({"schema":1,"anchor":[0,64,0],"radius":16,"min_y":64,"max_y":64,"max_targets":cap}),
        None,
    )
    .unwrap();
}
fn predicted(e: &mut Engine, targets: Value, pending: bool, revision: u64) -> Value {
    let mut v = envelope();
    v["source"] = json!({"world":"1","replay":"4","dimension":"minecraft:overworld"});
    v["revision"] = json!(revision.to_string());
    v["pending"] = json!(pending);
    v["targets"] = targets;
    e.dispatch(4, &v, None).unwrap()
}
fn snapshot(e: &mut Engine) -> Value {
    e.dispatch(7, &json!({"schema":1}), None).unwrap()
}

#[test]
fn wall_goto_keeps_executor_aim_when_host_restores_the_user_camera() {
    let mut engine = Engine::new();
    let host = Paper::new();
    host.real_rays.set(true);
    engine
        .dispatch(
            1,
            &json!({"schema":1,"settings":{"allowSprint":false}}),
            None,
        )
        .unwrap();
    for y in 64..=66 {
        for z in -4..=4 {
            host.set([4, y, z], 1, false, 1);
        }
    }
    let mut command = envelope();
    command["goal"] = json!([8, 64, 0]);
    engine.dispatch(2, &command, None).unwrap();
    let mut position = [0.5, 64.0, 0.5];
    let mut revision = 1u64;
    let mut mining = HashMap::<[i32; 3], f64>::new();
    let mut broken = Vec::new();
    let mut last = Value::Null;
    for tick in 1..=300 {
        // The real host applies engine aim during its input tick, then restores
        // the user's south-facing camera. Next frame is never the prior aim.
        let mut request = frame(tick);
        request["player"]["position"] = json!(position);
        request["player"]["yaw"] = json!(0);
        request["player"]["pitch"] = json!(0);
        request["block_revision"] = json!(revision.to_string());
        host.eye.set([position[0], position[1] + 1.62, position[2]]);
        last = engine.dispatch(5, &request, Some(&host)).unwrap();
        if !last["attack"].is_null() {
            let target = protocol::pos(&last["attack"]["pos"]).unwrap();
            let progress = mining.entry(target).or_default();
            *progress += 0.2;
            if *progress >= 1.0 {
                host.set(target, 0, false, 0);
                broken.push(target);
                revision += 1;
            }
        }
        let bits = last["movement"].as_u64().unwrap();
        let yaw = last["yaw"].as_f64().unwrap_or(0.0).to_radians();
        let forward = f64::from((bits & 1 != 0) as u8) - f64::from((bits & 2 != 0) as u8);
        let side = f64::from((bits & 8 != 0) as u8) - f64::from((bits & 4 != 0) as u8);
        let mut next = position;
        next[0] += (-yaw.sin() * forward + yaw.cos() * side) * 0.21;
        next[2] += (yaw.cos() * forward + yaw.sin() * side) * 0.21;
        let blocked = ((next[0] - 0.3).floor() as i32..=(next[0] + 0.3).floor() as i32).any(|x| {
            ((next[2] - 0.3).floor() as i32..=(next[2] + 0.3).floor() as i32)
                .any(|z| (64..=65).any(|y| host.block([x, y, z]).unwrap().state_id != 0))
        });
        if !blocked {
            position = next;
        }
        if last["active"] == false {
            break;
        }
    }
    assert!(
        broken.contains(&[4, 64, 0]) && broken.contains(&[4, 65, 0]),
        "position={position:?}; broken={broken:?}; {last}"
    );
    assert_eq!(
        last["detail"], "goal reached",
        "position={position:?}; {last}"
    );
    assert_eq!(position.map(|v| v.floor() as i32), [8, 64, 0]);
}

#[test]
fn failed_segment_replans_from_current_feet_and_preserves_its_diagnostic() {
    let mut engine = Engine::new();
    let host = Paper::new();
    let mut command = envelope();
    command["goal"] = json!([12, 64, 0]);
    engine.dispatch(2, &command, None).unwrap();
    let mut initial = Value::Null;
    let mut started_tick = 0;
    for tick in 1..=32 {
        started_tick = tick;
        initial = engine.dispatch(5, &frame(tick), Some(&host)).unwrap();
        if initial["movement"].as_u64().unwrap_or(0) != 0 {
            break;
        }
    }
    assert_ne!(initial["movement"], json!(0), "{initial}");
    assert_eq!(initial["draw"]["path"][0], json!([0, 64, 0]));

    // A server correction moves us outside the old segment. Upstream cancels
    // that executor, but the unchanged navigation goal still needs a new path.
    let mut moved = frame(started_tick + 1);
    moved["player"]["position"] = json!([0.5, 64.0, 4.5]);
    let failed = engine.dispatch(5, &moved, Some(&host)).unwrap();
    assert_eq!(failed["active"], true, "{failed}");
    assert_eq!(failed["movement"], 0);
    assert!(failed["attack"].is_null());
    let detail = failed["detail"].as_str().unwrap();
    assert!(detail.contains("replanning segment 1/3"), "{failed}");
    assert!(detail.contains("too far from path"), "{failed}");
    assert_eq!(snapshot(&mut engine)["detail"], failed["detail"]);

    let mut replanned = Value::Null;
    for tick in started_tick + 2..=started_tick + 32 {
        moved["tick"] = json!(tick.to_string());
        replanned = engine.dispatch(5, &moved, Some(&host)).unwrap();
        if replanned["movement"].as_u64().unwrap_or(0) != 0 {
            break;
        }
    }
    assert_eq!(replanned["draw"]["path"][0], json!([0, 64, 4]));
    assert_eq!(
        replanned["draw"]["path"].as_array().unwrap().last(),
        Some(&json!([12, 64, 0]))
    );
    assert_ne!(replanned["movement"], 0, "{replanned}");
    assert_eq!(replanned["active"], true);
}

#[test]
fn failed_segment_retries_remain_bounded_across_block_revisions() {
    let mut engine = Engine::new();
    let host = Paper::new();
    engine
        .dispatch(
            1,
            &json!({"schema":1,"settings":{"allowSprint":false,"movementTimeoutTicks":0}}),
            None,
        )
        .unwrap();
    let mut command = envelope();
    command["goal"] = json!([12, 64, 0]);
    engine.dispatch(2, &command, None).unwrap();
    let mut replans = Vec::new();
    let mut last = Value::Null;
    for tick in 1..=80 {
        // Inputs are available but the player is stuck; unrelated world updates
        // must not reset the failed-segment counter and produce an endless job.
        let mut request = frame(tick);
        request["block_revision"] = json!(tick.to_string());
        last = engine.dispatch(5, &request, Some(&host)).unwrap();
        let detail = last["detail"].as_str().unwrap().to_owned();
        if detail.starts_with("replanning segment") && !replans.contains(&detail) {
            replans.push(detail);
        }
        if last["active"] == false {
            break;
        }
    }
    assert_eq!(replans.len(), 3, "{replans:?}; {last}");
    assert_eq!(last["active"], false, "{last}");
    let detail = last["detail"].as_str().unwrap();
    assert!(
        detail.contains("retry budget exhausted (3 replans)"),
        "{last}"
    );
    assert!(
        detail.contains("This movement has taken too long"),
        "{last}"
    );
    assert_eq!(last["movement"], 0);
    assert!(last["attack"].is_null());
    assert!(last["yaw"].is_null());
}

#[test]
fn real_executor_drives_flat_goto_and_cancels_immediately() {
    let mut e = Engine::new();
    let host = Paper::new();
    let mut go = envelope();
    go["goal"] = json!([4, 64, 0]);
    e.dispatch(2, &go, None).unwrap();
    let mut a = Value::Null;
    let mut started_tick = 0;
    for tick in 1..=32 {
        started_tick = tick;
        a = e.dispatch(5, &frame(tick), Some(&host)).unwrap();
        if a["movement"].as_u64().unwrap_or(0) != 0 {
            break;
        }
    }
    assert!(!a["draw"]["path"].as_array().unwrap().is_empty());
    assert_ne!(a["movement"], json!(0), "{a}");
    e.dispatch(6, &json!({"schema":1,"reason":"panic"}), None)
        .unwrap();
    let c = e
        .dispatch(5, &frame(started_tick + 1), Some(&host))
        .unwrap();
    assert_eq!(c["movement"], 0);
    assert!(c["attack"].is_null());
    assert_eq!(c["active"], false);
}

#[test]
fn slow_host_search_yields_resumes_and_can_be_cancelled_between_slices() {
    struct SlowHost {
        paper: Paper,
        slow: Cell<bool>,
    }
    impl Host for SlowHost {
        fn block(&self, pos: [i32; 3]) -> protocol::Result<BlockReadV1> {
            if self.slow.get() {
                std::thread::sleep(Duration::from_millis(1));
            }
            self.paper.block(pos)
        }
        fn query(&self, operation: u32, request: &Value) -> protocol::Result<Value> {
            self.paper.query(operation, request)
        }
    }
    let host = SlowHost {
        paper: Paper::new(),
        slow: Cell::new(true),
    };
    let mut engine = Engine::new();
    let mut command = envelope();
    command["goal"] = json!([40, 64, 0]);
    engine.dispatch(2, &command, None).unwrap();
    let began = Instant::now();
    let first = engine.dispatch(5, &frame(1), Some(&host)).unwrap();
    // A slow node can exceed the small slice, but the old 2,048-node batch
    // cannot be allowed to block this call until the distant goal is found.
    assert!(began.elapsed() < Duration::from_millis(250));
    assert_ne!(
        first["draw"]["path"].as_array().unwrap().last(),
        Some(&json!([40, 64, 0]))
    );
    let mut continued = first;
    for tick in 2..=9 {
        continued = engine.dispatch(5, &frame(tick), Some(&host)).unwrap();
    }
    // Reaching beyond bestSoFar's minimum distance with one slow node per
    // slice proves the open set progressed across calls instead of restarting.
    assert!(
        continued["draw"]["path"].as_array().unwrap().len() >= 6,
        "{continued}"
    );
    host.slow.set(false);
    for tick in 10..=60 {
        continued = engine.dispatch(5, &frame(tick), Some(&host)).unwrap();
        if continued["movement"].as_u64().unwrap_or(0) != 0 {
            break;
        }
    }
    assert_ne!(continued["movement"], json!(0), "{continued}");
    assert_eq!(
        continued["draw"]["path"].as_array().unwrap().last(),
        Some(&json!([40, 64, 0]))
    );
    engine
        .dispatch(
            6,
            &json!({"schema":1,"reason":"cancel between slices"}),
            None,
        )
        .unwrap();
    let cancelled = engine.dispatch(5, &frame(61), Some(&host)).unwrap();
    assert_eq!(cancelled["active"], false);
    assert_eq!(cancelled["movement"], 0);
    assert!(cancelled["attack"].is_null());
    assert!(cancelled["draw"]["path"].as_array().unwrap().is_empty());
}

#[test]
fn long_paths_bound_post_processing_and_continue_with_original_goal() {
    let mut e = Engine::new();
    let host = Paper::new();
    let mut go = envelope();
    go["goal"] = json!([300, 64, 0]);
    e.dispatch(2, &go, None).unwrap();
    // Searching is cooperative: wait until the executor actually starts,
    // rather than treating the first best-so-far drawing as a finished path.
    let mut first = Value::Null;
    let mut tick = 0;
    for next_tick in 1..=60 {
        tick = next_tick;
        first = e.dispatch(5, &frame(tick), Some(&host)).unwrap();
        if first["movement"].as_u64().unwrap_or(0) != 0 {
            break;
        }
    }
    assert_ne!(first["movement"], json!(0), "{first}");
    let path = first["draw"]["path"].as_array().unwrap();
    assert!(path.len() <= 256);
    assert_eq!(first["draw"]["goal"], json!([300, 64, 0]));
    assert_ne!(path.last(), Some(&json!([300, 64, 0])));
    let mut next = frame(tick + 1);
    next["player"]["position"] = json!([255.5, 64.0, 0.5]);
    e.dispatch(5, &next, Some(&host)).unwrap();
    let mut continued = Value::Null;
    for next_tick in tick + 2..=tick + 30 {
        next["tick"] = json!(next_tick.to_string());
        continued = e.dispatch(5, &next, Some(&host)).unwrap();
        if continued["draw"]["path"].as_array().unwrap().last() == Some(&json!([300, 64, 0])) {
            break;
        }
    }
    assert_eq!(continued["draw"]["goal"], json!([300, 64, 0]));
    assert_eq!(
        continued["draw"]["path"].as_array().unwrap().last(),
        Some(&json!([300, 64, 0]))
    );
}

#[test]
fn executor_sprint_decision_is_transported_on_and_off() {
    for (allow, food, expected) in [(true, 20, true), (false, 20, false), (true, 6, false)] {
        let mut e = Engine::new();
        let host = Paper::new();
        e.dispatch(
            1,
            &json!({"schema":1,"settings":{"allowSprint":allow}}),
            None,
        )
        .unwrap();
        let mut command = envelope();
        command["goal"] = json!([12, 64, 0]);
        e.dispatch(2, &command, None).unwrap();
        let mut moved = false;
        for tick in 1..=5 {
            let mut f = frame(tick);
            f["player"]["food"] = json!(food);
            f["player"]["sprinting"] = json!(true);
            let v = e.dispatch(5, &f, Some(&host)).unwrap();
            let bits = v["movement"].as_u64().unwrap();
            if bits & 15 != 0 {
                assert_eq!(bits & 64 != 0, expected, "{v}");
                moved = true;
                break;
            }
        }
        assert!(moved);
    }
}

#[test]
fn actual_prediction_air_waits_until_server_settles() {
    let mut e = Engine::new();
    configure(&mut e, 4);
    let host = Paper::new();
    host.set(TARGET, 2, false, 2);
    predicted(&mut e, json!([{"pos":TARGET,"kind":6}]), false, 1);
    let a = e.dispatch(5, &frame(1), Some(&host)).unwrap();
    assert_eq!(a["attack"]["pos"], json!(TARGET));
    host.set(TARGET, 0, true, 2);
    let b = e.dispatch(5, &frame(2), Some(&host)).unwrap();
    assert!(b["attack"].is_null());
    assert_eq!(b["counters"]["mined"], 0);
    assert!(b["events"].as_array().unwrap().is_empty());
    host.set(TARGET, 0, false, 0);
    let c = e.dispatch(5, &frame(3), Some(&host)).unwrap();
    assert_eq!(c["counters"]["mined"], 1);
    assert_eq!(c["events"][0]["outcome"], "mined");
    assert_eq!(c["events"][0]["source"]["replay"], "4");
    assert!(snapshot(&mut e)["events"].as_array().unwrap().is_empty());
}

#[test]
fn verified_ore_adjacent_to_lava_is_not_attacked() {
    let mut e = Engine::new();
    configure(&mut e, 1);
    let host = Paper::new();
    host.set(TARGET, 2, false, 2);
    host.set([2, 64, 0], 5, false, 5);
    predicted(&mut e, json!([{"pos":TARGET,"kind":6}]), false, 1);
    let v = e.dispatch(5, &frame(1), Some(&host)).unwrap();
    assert!(v["attack"].is_null());
    assert_eq!(v["counters"]["mined"], 0);
    assert_eq!(v["events"][0]["outcome"], "rejected");
}

#[test]
fn masked_target_is_preserved_and_never_attacked_before_reveal() {
    let mut e = Engine::new();
    configure(&mut e, 4);
    let host = Paper::new();
    host.set(TARGET, 1, false, 1);
    predicted(&mut e, json!([{"pos":TARGET,"kind":6}]), false, 1);
    for tick in 1..5 {
        let value = e.dispatch(5, &frame(tick), Some(&host)).unwrap();
        assert_eq!(value["draw"]["target"], json!(TARGET));
        assert!(value["attack"].is_null());
        assert_eq!(value["counters"]["mismatch"], 0);
    }
    host.set(TARGET, 2, false, 2);
    let value = e.dispatch(5, &frame(5), Some(&host)).unwrap();
    assert_eq!(value["attack"]["pos"], json!(TARGET));
}

#[test]
fn covered_prediction_approaches_exposing_stance_instead_of_accepting_current_distance_two_cell() {
    let mut e = Engine::new();
    configure(&mut e, 1);
    let host = Paper::new();
    host.real_rays.set(true);
    let target = [2, 64, 0];
    host.set(target, 1, false, 1);
    host.set([1, 64, 0], 1, false, 1);
    host.set([1, 65, 0], 1, false, 1);
    predicted(&mut e, json!([{"pos":target,"kind":6}]), false, 1);
    let mut f = frame(1);
    let mut progressed = false;
    for tick in 1..=12 {
        f["tick"] = json!(tick.to_string());
        let v = e.dispatch(5, &f, Some(&host)).unwrap();
        if let Some(yaw) = v["yaw"].as_f64() {
            f["player"]["yaw"] = json!(yaw);
        }
        if let Some(pitch) = v["pitch"].as_f64() {
            f["player"]["pitch"] = json!(pitch);
        }
        if !v["attack"].is_null() {
            let pos = protocol::pos(&v["attack"]["pos"]).unwrap();
            assert_ne!(pos, target);
            assert!([[1, 64, 0], [1, 65, 0]].contains(&pos), "{v}");
            assert_eq!(v["counters"]["mined"], 0);
            progressed = true;
            break;
        }
        if v["movement"].as_u64().unwrap_or(0) & 15 != 0 {
            assert_eq!(v["counters"]["mined"], 0);
            progressed = true;
            break;
        }
    }
    assert!(
        progressed,
        "an occluding wall requires navigation to an exposing stance or clearing the cover"
    );
}

#[test]
fn visible_mine_uses_loaded_live_state_and_same_confirmation_process() {
    let mut e = Engine::new();
    configure(&mut e, 1);
    let host = Paper::new();
    host.set(TARGET, 2, false, 2);
    let mut command = envelope();
    command["blocks"] = json!(["minecraft:diamond_ore"]);
    e.dispatch(3, &command, None).unwrap();
    let v = e.dispatch(5, &frame(1), Some(&host)).unwrap();
    assert_eq!(v["attack"]["pos"], json!(TARGET));
    host.set(TARGET, 0, false, 0);
    let v = e.dispatch(5, &frame(2), Some(&host)).unwrap();
    assert_eq!(v["counters"]["mined"], 1);
}

#[test]
fn empty_predicted_queue_waits_bounded_and_never_scans_for_other_ores() {
    let mut e = Engine::new();
    configure(&mut e, 1);
    let host = Paper::new();
    host.set(TARGET, 2, false, 2);
    predicted(&mut e, json!([]), true, 1);
    let v = e.dispatch(5, &frame(1), Some(&host)).unwrap();
    assert_eq!(v["status"], "Waiting for seed/replay");
    assert_eq!(host.reads.get(), 0);
    let v = e.dispatch(5, &frame(201), Some(&host)).unwrap();
    assert_eq!(v["active"], false);
    assert_eq!(v["movement"], 0);
}

#[test]
fn max_targets_applies_to_whole_run_and_same_batch_cannot_restart_it() {
    let mut e = Engine::new();
    configure(&mut e, 1);
    let host = Paper::new();
    let targets = json!([{"pos":TARGET,"kind":6},{"pos":[2,64,0],"kind":6}]);
    predicted(&mut e, targets.clone(), false, 1);
    let v = e.dispatch(5, &frame(1), Some(&host)).unwrap();
    assert_eq!(v["counters"]["skipped"], 1);
    predicted(&mut e, targets.clone(), false, 2);
    let v = e.dispatch(5, &frame(2), Some(&host)).unwrap();
    assert_eq!(v["active"], false);
    assert_eq!(v["counters"]["attempted"], 1);
    let v = predicted(&mut e, targets, false, 3);
    assert_eq!(v["active"], false);
}

#[test]
fn missing_tool_and_allow_break_false_end_the_job_without_attack() {
    for allow_break in [true, false] {
        let mut e = Engine::new();
        configure(&mut e, 1);
        let host = Paper::new();
        host.set(TARGET, 2, false, 2);
        host.harvest.set(!allow_break);
        e.dispatch(
            1,
            &json!({"schema":1,"settings":{"allowBreak":allow_break}}),
            None,
        )
        .unwrap();
        predicted(&mut e, json!([{"pos":TARGET,"kind":6}]), false, 1);
        let v = e.dispatch(5, &frame(1), Some(&host)).unwrap();
        assert_eq!(v["active"], false);
        assert!(v["attack"].is_null());
        assert_eq!(v["counters"]["mined"], 0);
    }
}

#[test]
fn stale_lease_source_and_host_failure_fail_closed() {
    let mut e = Engine::new();
    configure(&mut e, 1);
    let host = Paper::new();
    host.set(TARGET, 2, false, 2);
    predicted(&mut e, json!([{"pos":TARGET,"kind":6}]), false, 1);
    let mut f = frame(1);
    f["lease_generation"] = json!("99");
    let v = e.dispatch(5, &f, Some(&host)).unwrap();
    assert_eq!(v["active"], false);
    assert!(v["attack"].is_null());
    let mut e = Engine::new();
    configure(&mut e, 1);
    predicted(&mut e, json!([{"pos":TARGET,"kind":6}]), false, 1);
    let mut replacement = envelope();
    replacement["source"] = json!({"world":"1","replay":"5","dimension":"minecraft:overworld"});
    replacement["revision"] = json!("2");
    replacement["pending"] = json!(false);
    replacement["targets"] = json!([]);
    assert_eq!(e.dispatch(4, &replacement, None).unwrap()["active"], false);
    let mut e = Engine::new();
    configure(&mut e, 1);
    predicted(&mut e, json!([]), true, 1);
    host.fail.set(true);
    assert_eq!(e.dispatch(5, &frame(1), Some(&host)), Err(HOST_UNAVAILABLE));
    assert_eq!(snapshot(&mut e)["active"], false);
}

#[test]
fn json_limits_record_sizes_and_nonfinite_coordinates_are_rejected() {
    assert_eq!(std::mem::size_of::<BlockReadV1>(), 20);
    assert_eq!(std::mem::size_of::<HostV1>(), 32);
    assert_eq!(std::mem::size_of::<ApiV1>(), 32);
    assert_eq!(std::mem::size_of::<BytesV1>(), 16);
    assert!(protocol::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    assert!(protocol::parse(b"{\"schema\":2}").is_err());
    assert!(protocol::parse(b"{\"schema\":1,\"x\":1e400}").is_err());
    let mut f = frame(1);
    f["world"] = json!(1);
    assert!(Frame::read(&f).is_err());
    let mut f = frame(1);
    f["player"]["position"] = json!([null, 64, 0]);
    assert!(Frame::read(&f).is_err());
    let mut f = frame(1);
    f["player"]["pitch"] = json!(91);
    assert!(Frame::read(&f).is_err());
    let mut e = Engine::new();
    assert_eq!(
        e.dispatch(999, &json!({"schema":1}), None),
        Err(UNSUPPORTED)
    );
}
