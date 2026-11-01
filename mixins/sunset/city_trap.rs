// Original native city mining and acknowledged obsidian placement.
use super::sunset::{self, Canvas, Category, Module, hold};
use super::sunset_city_policy::{self as policy, Ledger, SIDES};
use super::sunset_combat_control as control;
use super::sunset_util::{self as util, Target};
use super::{Game, Screen};
use horizon_protocol::packets::{
    Empty,
    play::{self, Clientbound as C, Serverbound as S},
};
use horizon_world::registry::BlockStateId;
use std::{
    any::Any,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const CITY: &str = "Auto City";
const TRAP: &str = "Auto Trap";
type Session = (i32, super::Dim, Arc<dyn Any + Send + Sync>);

pub(super) fn defaults() -> Vec<Module> {
    vec![
    Module::new(CITY,Category::Combat,"Mines one visible surround wall of the best enemy in a hole with native tool progress, then turns off. Pauses for menus or emergency inventory; current server state confirms the opening.").bool("Notifications",true),
    Module::new(TRAP,Category::Combat,"Places obsidian around an enemy's feet and head. Top block includes the roof. Optional Finish roof jump adds an outside support and may make two native jumps.")
        .num("Range",4.0,1.0,6.0,0.5).bool("Rotate",true)
        .num("Blocks per tick",4.0,1.0,8.0,1.0).bool("Top block",true).bool("Finish roof jump",false).bool("Show target",true)

]
}

#[derive(Clone, Copy, PartialEq)]
struct Face {
    block: [i32; 3],
    normal: [i32; 3],
    point: [f64; 3],
}
#[derive(Clone)]
struct Mining {
    target: u32,
    feet: [i32; 3],
    block: [i32; 3],
    before: BlockStateId,
    tool: usize,
    started: u64,
    waiting: Option<u64>,
    server_air: bool,
    restore: Option<usize>,
}
#[derive(Default)]
struct State {
    session: Option<Session>,
    tick: u64,
    mine: Option<Mining>,
    city_finish: Option<control::Finish>,
    city_queued: Option<Instant>,
    trap_queued: Option<Instant>,
    trap_empty_warned:bool,
    trap_key: Option<(u32, [i32; 3])>,
    trap_target: Option<Target>,
    trap_plan: Vec<[i32; 3]>,
    ledger: Ledger,
    roof_jumps: policy::RoofJumps,
    generation: u64,
    reason: &'static str,
    placements: u64,
    mining_steps: u64,
    manual_cancel: bool,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}
fn same(game: &Game<'_>, old: &Session) -> bool {
    game.remote.as_ref().is_some_and(|r| {
        r.entity_id == old.0
            && game.dim == old.1
            && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ()
    })
}
fn session(game: &Game<'_>) -> Option<Session> {
    let r = game.remote.as_ref()?;
    Some((r.entity_id, game.dim, r.session.counters.clone()))
}

fn city_basic(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && game.native_playing()
        && game.remote.is_some()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && !game.spectator()
        && game.selected < 9
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && game.mobs.len() <= 512
        && game.crystals.len() <= 512
}
fn city_live(game:&Game<'_>)->bool {
    game.native_window()&&game.native_on(sunset::ID)&&game.title.is_none()&&game.remote.is_some()
        && !game.spectator()&&game.selected<9&&game.mobs.len()<=512&&game.crystals.len()<=512
        && game.player.as_ref().is_some_and(|p|p.health.health().is_finite()&&p.health.health()>0.0)
}
fn city_range(game:&mut Game<'_>)->Option<f64> {
    game.player.as_mut()?.attribute("minecraft:block_interaction_range").ok()
        .filter(|range|range.is_finite()&&*range>0.0&&*range<=64.0)
}
fn city_clicks(game:&Game<'_>)->bool {
    game.screen==Screen::Playing&&!sunset::gui_open()&&(clicks(game)||game.attack_clicks>0)
}
fn city_target(game:&Game<'_>,id:u32)->Option<Target> {
    let mob=game.mobs.iter().find(|mob|mob.id==id&&util::player_enemy(game,mob))?;
    let target=util::mob_target(mob);
    (policy::valid(target.pos)&&policy::valid(target.lo)&&policy::valid(target.hi)).then_some(target)
}
fn city_note(game:&mut Game<'_>,reason:&str) {
    if sunset::with(|s|s.module(CITY).is_some_and(|m|m.b("Notifications"))) {
        game.titles.set_overlay(horizon_client::text::Text::literal(reason),false);
    }
}

fn clicks(game: &Game<'_>) -> bool {
    game.left
        || game.right
        || game
            .physical_buttons
            .contains(&winit::event::MouseButton::Left)
        || game
            .physical_buttons
            .contains(&winit::event::MouseButton::Right)
}
fn hands_free(game: &Game<'_>) -> bool {
    game.using.is_none()
        && game.carried.is_none()
        && util::inventory_idle(game)
        && game.riding.is_none()
        && !sunset::with(|s| s.is_on("Blink"))
}
fn totem_due(game: &Game<'_>) -> bool {
    game.offhand.is_none()
        && sunset::with(|s| s.is_on("Auto Totem") || s.is_on("Auto Inventory Totem"))
        && game
            .inventory
            .iter()
            .take(36)
            .flatten()
            .any(|s| &*s.item == "minecraft:totem_of_undying" && s.count == 1)
}

pub(super) fn reset() {
    sunset::cancel_turns(CITY);
    sunset::cancel_turns(TRAP);
    control::release(TRAP);
    state(|s| {
        let generation = s.generation.wrapping_add(1);
        *s = State::default();
        s.generation = generation;
    });
}
fn restore_tool(game: &mut Game<'_>, mine: &Mining) {
    if let Some(slot) = mine.restore {
        if game.selected == mine.tool && slot < 9 {
            hold(game, slot);
        }
    }
}
fn city_current() -> bool {
    state(|s| s.city_finish.as_ref().is_some_and(|guard| guard.current()))
}
fn city_generation_current(generation: u64) -> bool {
    state(|s| {
        s.generation == generation && s.city_finish.as_ref().is_some_and(|guard| guard.current())
    })
}
// Native gameplay down/up can both arrive between ticks. Retain the press edge
// and invalidate queued work immediately; the next tick performs owned cleanup.
// Module-generated Punch/use packets do not pass through this input hook.
pub(super) fn user_mouse(game: &Game<'_>, button: winit::event::MouseButton, pressed: bool) {
    if !pressed
        || !matches!(button, winit::event::MouseButton::Left | winit::event::MouseButton::Right)
        || !game.native_window() || !game.native_on(sunset::ID)
        || !game.native_playing() || sunset::gui_open()
    { return; }
    let cancelled = state(|s| {
        if s.mine.is_none() || !s.session.as_ref().is_some_and(|old| same(game, old)) { return false; }
        s.manual_cancel = true;
        s.generation = s.generation.wrapping_add(1);
        s.city_queued = None;
        true
    });
    if cancelled { sunset::cancel_turns(CITY); }
}
fn pause_city(game: &mut Game<'_>) {
    sunset::cancel_turns(CITY);
    let (mine, guard) = state(|s| {
        s.generation = s.generation.wrapping_add(1);
        s.city_queued = None;
        let mine = s.mine.clone();
        if let Some(mine) = &mut s.mine {
            mine.restore = None;
        }
        (mine, s.city_finish.take())
    });
    if guard.as_ref().is_some_and(|guard| guard.current()) {
        if let Some(mine) = mine {
            super::remote::sunset_city_mining::stop(game, mine.block);
            restore_tool(game, &mine);
        }
    }
    drop(guard);
}
fn stop_city(game: &mut Game<'_>, reason: &'static str, disable: bool) {
    sunset::cancel_turns(CITY);
    let (mine, guard) = state(|s| {
        s.generation = s.generation.wrapping_add(1);
        s.city_queued = None;
        s.reason = reason;
        s.manual_cancel = false;
        (s.mine.take(), s.city_finish.take())
    });
    if guard.as_ref().is_some_and(|guard| guard.current()) {
        if let Some(mine) = &mine {
            super::remote::sunset_city_mining::stop(game, mine.block);
            restore_tool(game, mine);
        }
    }
    drop(guard);
    if disable {
        util::toggle_off(game, CITY);
        city_note(game,reason);
    }
}
pub(super) fn cancel(game: &mut Game<'_>) {
    let valid = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    let active = state(|s| s.mine.is_some() || s.city_queued.is_some());
    if valid {
        stop_city(game, "Cancelled", active);
    }
    reset();
    // A screen pause ends active one-shot work but is not a reconnect. Keep
    // the observed connection so an explicit GUI/chat enable can start once
    // that screen closes. Only a real session/dimension change disables saved on.
    if valid {
        state(|s| { s.session=session(game);s.tick=game.ticks; });
    }
}

fn opponents_with(game: &Game<'_>, _m: &Module, range: f64, only: Option<u32>) -> Vec<Target> {
    let Some(eye) = util::player_pos(game) else {
        return Vec::new();
    };
    let friends = sunset::with(|s| s.friends.clone());
    let mut found = Vec::with_capacity(8);
    for mob in &game.mobs {
        if only.is_some_and(|id| id != mob.id) {
            continue;
        }
        if mob.removed
            || !mob.alive()
            || mob.kind.id != "minecraft:player"
            || game
                .remote
                .as_ref()
                .is_some_and(|r| r.entity_id as u32 == mob.id)
            || !util::player_enemy(game,mob)
        {
            continue;
        }
        let Some(name) = game.remote_player_name(mob.id) else {
            continue;
        };
        if friends.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            continue;
        }
        let t = util::mob_target(mob);
        if !policy::valid(t.pos)
            || !policy::valid(t.lo)
            || !policy::valid(t.hi)
            || !policy::near(eye, t.pos, range)
        {
            continue;
        }
        found.push(t);
    }
    found.sort_unstable_by(|a, b| util::dist(eye, a.pos).total_cmp(&util::dist(eye, b.pos)));
    found.truncate(3);
    found
}
fn opponents(game: &Game<'_>, m: &Module, range: f64) -> Vec<Target> {
    opponents_with(game, m, range, None)
}
fn opponent(game: &Game<'_>, m: &Module, range: f64, id: u32) -> Option<Target> {
    opponents_with(game, m, range, Some(id)).into_iter().next()
}
fn same_hole(target: &Target, feet: [i32; 3]) -> bool {
    let current = target.pos.map(|n| n.floor() as i32);
    current[0] == feet[0]
        && current[2] == feet[2]
        && (i64::from(current[1]) - i64::from(feet[1])).abs() <= 1
}

fn city_reachable(game:&Game<'_>,block:[i32;3],range:f64)->bool {
    let Some(eye)=game.eye_position() else{return false;};
    let lo=block.map(f64::from);let hi=lo.map(|v|v+1.0);
    policy::box_distance(eye,lo,hi).is_some_and(|distance|distance<=range)
}
fn city_face(game:&Game<'_>,block:[i32;3],range:f64)->Option<Face> {
    if !range.is_finite()||range<=0.0||range>64.0||!game.world.loaded(block[0],block[2]){return None;}
    let eye=game.eye_position()?;
    let mut nearest=range*range;let mut best=None;
    for normal in [[0,-1,0],[0,1,0],[0,0,-1],[0,0,1],[-1,0,0],[1,0,0]] {
        let point=std::array::from_fn::<_,3,_>(|i|f64::from(block[i])+0.5+0.5*f64::from(normal[i]));
        let squared=(0..3).map(|i|(point[i]-eye[i]).powi(2)).sum::<f64>();
        if !squared.is_finite()||squared>nearest{continue;}
        let inside=std::array::from_fn::<_,3,_>(|i|point[i]-0.05*f64::from(normal[i]));
        let from=horizon_core::geometry::Vec3::new(eye[0],eye[1],eye[2]);
        let to=horizon_core::geometry::Vec3::new(inside[0],inside[1],inside[2]);
        let loaded=horizon_core::geometry::traverse_blocks(from,to,512,|p:horizon_core::position::BlockPos|
            (!game.world.loaded(p.x,p.z)).then_some(()));
        if !matches!(loaded,Ok(None)){continue;}
        let delta=std::array::from_fn::<_,3,_>(|i|inside[i]-eye[i]);
        let distance=delta.iter().map(|v|v*v).sum::<f64>().sqrt();
        if !distance.is_finite()||distance<=1e-8{continue;}
        let Some(hit)=game.clip_ray(eye,delta.map(|v|v/distance),distance) else{continue;};
        if hit.pos!=block||hit.face!=normal{continue;}
        nearest=squared;best=Some(Face{block,normal,point:hit.point});
    }
    best
}


fn safe_support(game: &Game<'_>, block: [i32; 3]) -> bool {
    let f = util::facts(game, block);
    if f.replaceable || f.block_entity || f.fluid_level > 0 {
        return false;
    }
    ![
        "CraftingTableBlock",
        "RespawnAnchorBlock",
        "AnvilBlock",
        "LoomBlock",
        "CartographyTableBlock",
        "StonecutterBlock",
        "GrindstoneBlock",
        "DoorBlock",
        "TrapDoorBlock",
        "FenceGateBlock",
        "ButtonBlock",
        "LeverBlock",
        "BedBlock",
        "ComposterBlock",
        "CakeBlock",
    ]
    .iter()
    .any(|name| f.has_class(name))
}

fn can_fill(game: &Game<'_>, cell: [i32; 3]) -> bool {
    let facts=util::facts(game,cell);
    (facts.fluid==super::Fluid::None || facts.fluid_level==8&&!facts.fluid_falling)
        && util::can_place(game, cell, true)
        && !game.crystals.iter().any(|c| {
            let p = c.position;
            p.x - 1.0 < f64::from(cell[0]) + 1.0
                && p.x + 1.0 > f64::from(cell[0])
                && p.y < f64::from(cell[1]) + 1.0
                && p.y + 2.0 > f64::from(cell[1])
                && p.z - 1.0 < f64::from(cell[2]) + 1.0
                && p.z + 1.0 > f64::from(cell[2])
        })
}
fn tool(game: &Game<'_>, block: [i32; 3]) -> Option<usize> {
    let facts = util::facts(game, block);
    if !facts.hardness.is_finite() || facts.hardness < 0.0 {
        return None;
    }
    let mut best = None;
    for slot in 0..9 {
        let Some(item) = game.inventory[slot].as_ref() else {
            continue;
        };
        if item.count == 0
            || !item.item.ends_with("_pickaxe")
            || item.extra.as_ref().is_some_and(|s| !s.other.is_empty())
            || !game.items().correct_for_drops(Some(item), &facts.name)
        {
            continue;
        }
        let Some(info) = game.items().get(&item.item) else {
            continue;
        };
        if info.max_damage > 0
            && item
                .damage
                .saturating_add(info.tool_damage_per_block.unwrap_or(1))
                >= info.max_damage
        {
            continue;
        }
        let speed = game.items().mining_speed(Some(item), &facts.name);
        if speed.is_finite() && speed > 0.0 && best.is_none_or(|(_, old)| speed > old) {
            best = Some((slot, speed));
        }
    }
    best.map(|(slot, _)| slot)
}
fn in_hole(game: &Game<'_>, feet: [i32; 3]) -> bool {
    SIDES.iter().all(|d| {
        let p = std::array::from_fn(|i| feet[i] + d[i]);
        game.world.loaded(p[0], p[2]) && util::facts(game, p).explosion_resistance >= 600.0
    })
}

fn city_score(game:&mut Game<'_>,_m:&Module,target:&Target,block:[i32;3],budget:&mut super::sunset_explosive_policy::Budget)->Option<f64> {
    let blast=super::sunset_explosive_damage::Blast::explosion([
        f64::from(block[0])+0.5,f64::from(block[1]),f64::from(block[2])+0.5],6.0).ignoring(block);
    let own=super::sunset_explosive_damage::local_damage(game,blast,budget)?.estimate;
    let damage=super::sunset_explosive_tactics::friend_damage(game,target,blast,budget)?;
    let player=game.player.as_ref()?;let health=f64::from(player.health.health()+player.health.absorption());
    if super::sunset_explosive_tactics::endangered(own,health,game.creative)||!damage.is_finite()||damage<0.0{return None;}
    let value=damage-0.5*own;
    (value>0.0).then_some(value)
}

fn select_city(game: &mut Game<'_>, m: &Module, tick: u64) -> Option<Mining> {
    if !super::sunset_explosive_tactics::refresh(game){return None;}
    let range=city_range(game)?;
    let (targets,_)=super::sunset_explosive_tactics::players(game)?;
    let mut budget = super::sunset_explosive_policy::Budget::new(24);
    let mut best = None;
    for target in targets {
        let feet = target.pos.map(|n| n.floor() as i32);
        if !in_hole(game, feet) {
            continue;
        }
        for d in SIDES {
            let block = std::array::from_fn(|i| feet[i] + d[i]);
            let base = [block[0], block[1] - 1, block[2]];
            if !matches!(
                &*util::facts(game, base).name,
                "minecraft:obsidian" | "minecraft:bedrock"
            ) {
                continue;
            }
            let Some(slot) = tool(game, block) else {
                continue;
            };
            if city_face(game, block, range).is_none() {
                continue;
            }
            let Some(score) = city_score(game, m, &target, block, &mut budget) else {
                continue;
            };
            if best.as_ref().is_none_or(|(old, _)| score > *old) {
                best = Some((
                    score,
                    Mining {
                        target: target.id,
                        feet,
                        block,
                        before: game.world.block(block[0], block[1], block[2]),
                        tool: slot,
                        started: tick,
                        waiting: None,
                        server_air: false,
                        restore: None,
                    },
                ));
            }
        }
    }
    best.map(|(_, mine)| mine)
}

fn city_tick(game: &mut Game<'_>, m: &Module, tick: u64) {
    if city_clicks(game) || state(|s| s.manual_cancel) {
        stop_city(game, "Stopped for manual input", true);
        return;
    }
    if city_current() {
        let _ = control::claim(CITY);
    }
    if !city_current() && super::remote::sunset_city_mining::busy(game) {
        stop_city(game, "Existing mining has priority", true);
        return;
    }
    if state(|s| s.mine.is_none()) {
        let Some(mine) = select_city(game, m, tick) else {
            stop_city(game, "No safe reachable city block", true);
            return;
        };
        state(|s| s.mine = Some(mine));
    }
    let Some(mut mine) = state(|s| s.mine.clone()) else {
        return;
    };
    if mine.server_air {
        let named=game.remote_player_name(mine.target).map(|name|format!("Opened {name}'s surround"));
        stop_city(game,"Surround opening confirmed",true);
        if let Some(message)=named{city_note(game,&message);}
        return;
    }
    if tick < mine.started || tick.saturating_sub(mine.started) >= 200 {
        stop_city(game, "Mining timeout", true);
        return;
    }
    if city_current()
        && super::remote::sunset_city_mining::progress(game, mine.block).is_some()
        && game.selected != mine.tool
    {
        stop_city(game, "Stopped for hotbar input", true);
        return;
    }
    let target = city_target(game,mine.target);
    let Some(target) = target else {
        stop_city(game, "Target unavailable", true);
        return;
    };
    if !same_hole(&target, mine.feet) {
        stop_city(game, "Target left the hole", true);
        return;
    }
    if let Some(waiting) = mine.waiting {
        if tick.saturating_sub(waiting) >= 30 {
            stop_city(game, "No server opening acknowledgement", true);
        }
        return;
    }
    let current = game
        .world
        .block(mine.block[0], mine.block[1], mine.block[2]);
    if current != mine.before {
        if super::remote::sunset_city_mining::predicted(game, mine.block) {
            mine.waiting = Some(tick);
            state(|s| s.mine = Some(mine));
        } else {
            stop_city(game, "Block changed before confirmation", true);
        }
        return;
    }
    let Some(slot) = tool(game, mine.block) else {
        stop_city(game, "No usable pickaxe", true);
        return;
    };
    if slot != mine.tool {
        stop_city(game, "Mining tool changed", true);
        return;
    }
    let Some(range)=city_range(game) else {stop_city(game,"Unknown native interaction reach",true);return;};
    if !city_reachable(game,mine.block,range){stop_city(game,"Block went out of reach",true);return;}
    if game.riding.is_some()||game.player.as_ref().is_some_and(|p|p.fall_flying||p.auto_spin_attack_ticks>0||p.swimming){stop_city(game,"Cannot aim while flying, swimming or riding",true);return;}
    if !city_basic(game)||!hands_free(game)||totem_due(game)||!control::available(CITY)||super::sunset_render::camera_takes_mouse() {
        pause_city(game);return;
    }
    let Some(face) = city_face(game,mine.block,range) else {
        stop_city(game, "Lost block sight or reach", true);
        return;
    };
    if city_current()
        && state(|s| {
            s.city_queued
                .is_some_and(|at| at.elapsed() <= Duration::from_millis(200))
        })
    {
        control::claim(CITY);
        return;
    }
    let eye=game.eye_position().unwrap_or([f64::NAN;3]);
    let desired=sunset::look_at(eye,face.point);
    let (yaw,pitch)=super::sunset_aura::next_look(game,desired,0.0);
    let held=game.clip_ray(eye,super::view_vector(yaw,pitch),range).is_some_and(|hit|hit.pos==mine.block);
    if !held {sunset::turn(CITY,yaw,pitch);return;}
    sunset::cancel_turns(CITY);
    let continuing = city_current();
    if !control::claim(CITY) {
        return;
    }
    if !continuing {
        mine.restore = Some(game.selected);
        let guard = control::finish(CITY);
        if !guard.current() {
            return;
        }
        state(|s| {
            s.generation = s.generation.wrapping_add(1);
            s.city_finish = Some(guard);
            s.mine = Some(mine.clone());
        });
    } else if mine.restore.is_none() {
        mine.restore = Some(game.selected);
        state(|s| s.mine = Some(mine.clone()));
    }
    let Some(connection) = session(game) else {
        return;
    };
    let snapshot = m.clone();
    let selected_before_queue = game.selected;
    let original_tool = game.inventory[mine.tool].clone();
    let queued = Instant::now();
    let generation = state(|s| {
        s.city_queued = Some(queued);
        s.generation
    });
    if !sunset::try_turn_then(CITY, yaw, pitch, move |game| {
        if !city_generation_current(generation) {
            return;
        }
        state(|s| s.city_queued = None);
        if same(game, &connection) && game.selected != selected_before_queue {
            stop_city(game, "Stopped for hotbar input", true);
            return;
        }
        if queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !city_basic(game)
            || !hands_free(game)
            || city_clicks(game)
            || totem_due(game)
            || !city_generation_current(generation)
            || game.inventory[mine.tool] != original_tool
            || !sunset::with(|s| s.module(CITY).is_some_and(|m| m.on))
            || game
                .world
                .block(face.block[0], face.block[1], face.block[2])
                != mine.before
        {
            return;
        }
        let Some(target)=city_target(game,mine.target) else{return;};
        if !same_hole(&target, mine.feet) {
            return;
        }
        let Some(range)=city_range(game) else{return;};
        let Some(fresh)=city_face(game,face.block,range) else {
            return;
        };
        if fresh.normal != face.normal || !policy::near(fresh.point, face.point, 0.1) {
            return;
        }
        hold(game, mine.tool);
        let mut safety_budget = super::sunset_explosive_policy::Budget::new(2);
        if city_score(game, &snapshot, &target, mine.block, &mut safety_budget).is_none() {
            stop_city(game, "Current health or equipment safety changed", true);
            return;
        }
        if !city_generation_current(generation) {
            return;
        }
        if super::sunset_melee_assist::acted(game){return;}
        let Some((sent_yaw,sent_pitch))=super::sunset_combat_wire::sent_look(game) else{return;};
        let Some(now_eye)=game.eye_position() else{return;};
        let Some(actual)=game.clip_ray(now_eye,super::view_vector(sent_yaw,sent_pitch),range).filter(|hit|hit.pos==mine.block) else{return;};
        if super::remote::sunset_city_mining::step_with_range(game,mine.block,actual.face,sent_yaw,sent_pitch,range) {
            super::sunset_melee_assist::note_action(game);
            game.swing_arm();
            game.send(S::Punch(Empty));
            state(|s| s.mining_steps = s.mining_steps.saturating_add(1));
            if super::remote::sunset_city_mining::predicted(game, face.block) {
                state(|s| {
                    if let Some(m) = &mut s.mine {
                        m.waiting = Some(game.ticks);
                    }
                });
            }
        }
    }) {
        pause_city(game);
    }
}

// Hold input alone is not a consumed action. Respect the actual native
// mining/use state and the final forwarded-action tick instead.
fn trap_hands_free(game:&Game<'_>)->bool {
    hands_free(game) && game.mining.is_none() && !super::remote::sunset_city_mining::busy(game)
        && !super::sunset_player::gap_eating() && !super::sunset_render::camera_takes_mouse()
        && !game.keys.contains(&winit::keyboard::KeyCode::KeyQ)
        && !game.keys.contains(&winit::keyboard::KeyCode::KeyF)
}
fn face_aim_with(game:&Game<'_>,block:[i32;3],range:f64,desired:Option<[i32;3]>,budget:&mut usize)->Option<Face> {
    if !game.world.loaded(block[0],block[2]){return None;}
    let eye=game.eye_position()?;let mut best=None;
    for normal in [[0,-1,0],[0,1,0],[0,0,-1],[0,0,1],[-1,0,0],[1,0,0]] {
        if desired.is_some_and(|face|face!=normal){continue;}
        let point=std::array::from_fn(|i|f64::from(block[i])+0.5+0.5*f64::from(normal[i]));
        if !policy::near(eye,point,range){continue;}
        if *budget==0{break;}*budget-=1;
        let inside=std::array::from_fn::<_,3,_>(|i|point[i]-0.05*f64::from(normal[i]));
        let delta=std::array::from_fn::<_,3,_>(|i|inside[i]-eye[i]);
        let length=delta.iter().map(|v|v*v).sum::<f64>().sqrt();
        if !length.is_finite()||length<=1e-8{continue;}
        let Some(hit)=game.clip_ray(eye,delta.map(|v|v/length),length) else{continue;};
        if hit.pos!=block||hit.face!=normal{continue;}
        let distance=util::dist(eye,point);
        if best.as_ref().is_none_or(|(old,_)|distance<*old){best=Some((distance,Face{block,normal,point}));}
    }
    best.map(|(_,face)|face)
}
fn support(game:&Game<'_>,cell:[i32;3],range:f64,budget:&mut usize)->Option<Face> {
    // Down, North, South, East, West, Up: the exact reference preference.
    for toward in [[0,-1,0],[0,0,-1],[0,0,1],[1,0,0],[-1,0,0],[0,1,0]] {
        let block=std::array::from_fn(|i|cell[i]+toward[i]);
        if !game.world.loaded(block[0],block[2])||!safe_support(game,block){continue;}
        let normal=toward.map(|v|-v);
        if let Some(face)=face_aim_with(game,block,range,Some(normal),budget){return Some(face);}
        if *budget==0{return None;}
    }
    None
}
fn trap_contact(game:&Game<'_>,face:Face,range:f64)->Option<[f64;3]> {
    let eye=game.eye_position()?;let (yaw,pitch)=super::sunset_combat_wire::sent_look(game)?;
    let hit=game.clip_ray(eye,super::view_vector(yaw,pitch),range)?;
    (hit.pos==face.block&&hit.face==face.normal).then_some(hit.point)
}

fn trap_tick(game: &mut Game<'_>, m: &Module, tick: u64) {
    state(|s| s.ledger.expire(tick));
    if !trap_hands_free(game) || super::sunset_melee_assist::acted(game) || totem_due(game) || !control::available(TRAP) {
        sunset::cancel_turns(TRAP);
        state(|s| s.trap_queued = None);
        control::release(TRAP);
        return;
    }
    if state(|s| {
        s.trap_queued
            .is_some_and(|at| at.elapsed() <= Duration::from_millis(200))
    }) {
        control::claim(TRAP);
        return;
    }
    sunset::cancel_turns(TRAP);
    state(|s| s.trap_queued = None);
    let Some(target) = opponents(game, m, m.n("Range") + 1.5).into_iter().next() else {
        state(|s| {
            s.trap_target = None;
            s.trap_plan.clear();
        });
        return;
    };
    let Some(range)=city_range(game) else{return;};
    let feet = target.pos.map(|n| n.floor() as i32);
    let key = (target.id, feet);
    let eye = game.eye_position().unwrap_or([f64::NAN; 3]);
    let Some(mut plan) = (if m.b("Finish roof jump") {policy::trap_cells_from(target.lo,target.hi,m.b("Top block"),eye)}else{policy::trap_cells(target.lo,target.hi,m.b("Top block"))})
    else {
        return;
    };
    let roof_y = (target.lo[1] + 0.01).floor() as i32 + 2;
    // Check the full lower ring before collision filtering. An empty cell
    // occupied by another player is not a completed support column.
    let lower_complete = plan.iter().filter(|p| p[1] < roof_y).all(|p| {
        let facts = util::facts(game, *p);
        game.world.loaded(p[0], p[2]) && !facts.replaceable && facts.full_collision
    });
    if m.b("Finish roof jump") {policy::order_trap(&mut plan,eye);} else {
        plan.sort_by(|a,b|a[1].cmp(&b[1]).then_with(||util::dist(eye,a.map(|v|f64::from(v)+0.5))
            .total_cmp(&util::dist(eye,b.map(|v|f64::from(v)+0.5)))));
    }
    plan.retain(|p| game.world.loaded(p[0], p[2]) && policy::near(eye,util::center_of(*p),range) && can_fill(game, *p));
    state(|s| {
        if s.trap_key != Some(key) {
            s.ledger.clear();
            s.roof_jumps.clear();
            s.trap_key = Some(key);
        }
        s.trap_target = Some(target.clone());
        s.trap_plan = plan.clone();
    });
    let Some(slot) = util::find_hotbar(game, |n| n == "obsidian") else {
        let warn=state(|s|{let warn=!s.trap_empty_warned;s.trap_empty_warned=true;warn});
        if warn{sunset::say(game,"Auto Trap: No obsidian in the hotbar");}return;
    };
    state(|s|s.trap_empty_warned=false);
    let mut selected = Vec::with_capacity(8);
    let count = game.inventory[slot]
        .as_ref()
        .map_or(0, |s| s.count as usize);
    let limit = (if m.b("Rotate") {
        1
    } else {
        m.n("Blocks per tick").clamp(1.0, 8.0) as usize
    })
    .min(if game.creative{8}else{count});
    if plan.is_empty() || limit == 0 {
        return;
    }
    let mut budget = 96;
    // A rotating start can erect the near column before farther faces are
    // reachable. Preserve far-to-near order at each height across ACK waits.
    for cell in plan.iter().take(36).copied() {
        if !selected.is_empty() {
            break;
        }
        let before = game.world.block(cell[0], cell[1], cell[2]).0;
        if !state(|s| s.ledger.can_try(cell, before)) {
            continue;
        }
        let Some(face) = support(game, cell, range, &mut budget) else {
            if budget == 0 {
                break;
            }
            continue;
        };
        if !super::remote::sunset_city_mining::preview(
            game,
            cell,
            face.block,
            face.normal,
            face.point,
            slot,
        ) {
            continue;
        }
        selected.push((cell, before, face));
    }
    if selected.is_empty() {
        let jump = m.b("Top block")
            && m.b("Finish roof jump")
            && lower_complete
            && eye[1] <= f64::from(roof_y) + 0.1
            && plan.iter().all(|p| p[1] == roof_y)
            && state(|s| s.ledger.waiting() == 0)
            && !game
                .physical_keys
                .contains(&winit::keyboard::KeyCode::Space)
            && game.player.as_ref().is_some_and(|p| {
                p.ground.on_ground
                    && !p.flying
                    && !p.fall_flying
                    && !p.fluids.was_touching_water
                    && !p.is_in_lava()
                    && p.last_climbable.is_none()
            });
        if jump && control::claim(TRAP) && state(|s| s.roof_jumps.request(tick)) {
            super::sunset_melee_assist::request_jump(TRAP);
        } else {
            control::release(TRAP);
        }
        return;
    }
    if m.b("Rotate") && trap_contact(game,selected[0].2,range).is_none() {
        if !super::sunset_explosive_tactics::fixed_look(game) {
            let desired=sunset::look_at(eye,selected[0].2.point);
            let look=super::sunset_aura::next_look(game,desired,0.0);sunset::turn(TRAP,look.0,look.1);
        }return;
    }
    if !control::claim(TRAP) {
        return;
    }
    let Some(connection) = session(game) else {
        control::release(TRAP);
        return;
    };
    let finish = control::finish(TRAP);
    if !finish.current() {
        return;
    }
    let snapshot = m.clone();
    let original = game.inventory[slot].clone();
    let queued = Instant::now();
    let generation = state(|s| {
        s.trap_queued = Some(queued);
        s.generation
    });
    let look=super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw,game.pitch));
    if !sunset::try_turn_then(TRAP, look.0, look.1, move |game| {
        let _finish = finish;
        if !_finish.current() || state(|s| s.generation != generation) {
            return;
        }
        state(|s| s.trap_queued = None);
        if queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !city_basic(game)
            || !trap_hands_free(game)
            || super::sunset_melee_assist::acted(game)
            || !_finish.current()
            || state(|s| s.generation != generation)
            || game.inventory[slot] != original
            || !sunset::with(|s| s.module(TRAP).is_some_and(|m| m.on && *m == snapshot))
        {
            return;
        }
        let fresh = opponent(game, &snapshot, snapshot.n("Range") + 1.5, target.id);
        if fresh.is_none_or(|t| !policy::near(t.pos, target.pos, 0.75)) {
            return;
        }
        let Some(range)=city_range(game) else{return;};
        let before_slot = game.selected;
        hold(game, slot);
        let mut budget=96;let mut placed=0;
        // Native use predicts each accepted block. Later cells in this same
        // bounded plan can therefore use the newly placed support naturally.
        for cell in plan.into_iter().take(36) {
            if placed>=limit{break;}
            let Some(eye)=game.eye_position() else{break;};
            if !policy::near(eye,util::center_of(cell),range)||!can_fill(game,cell){continue;}
            let before=game.world.block(cell[0],cell[1],cell[2]).0;
            if !state(|s|s.ledger.can_try(cell,before)){continue;}
            let Some(mut face)=support(game,cell,range,&mut budget) else{if budget==0{break;}continue;};
            if snapshot.b("Rotate") {
                let Some(point)=trap_contact(game,face,range) else{continue;};face.point=point;
            }
            if !super::remote::sunset_city_mining::preview(game,cell,face.block,face.normal,face.point,slot){continue;}
            if placed==0&&super::sunset_melee_assist::acted(game){break;}
            if !state(|s|s.ledger.sent(cell,before,game.ticks)){continue;}
            if super::remote::sunset_city_mining::place(game,cell,face.block,face.normal,face.point) {
                // Final accepted UseItemOn is recorded by combat_wire::accepted.
                // Native use already performed the local arm animation.
                game.send(S::Punch(Empty));placed+=1;
                state(|s|s.placements=s.placements.saturating_add(1));
            }
        }
        if game.selected == slot {
            hold(game, before_slot);
        }
    }) {
        state(|s| s.trap_queued = None);
    }
}

fn observed(game: &Game<'_>, pos: [i32; 3], id: u32) {
    let state_id = BlockStateId(id);
    if game.pack.assets.registry.state(state_id).is_none() {
        return;
    }
    state(|s| {
        s.ledger.observe(pos);
        if let Some(mine) = &mut s.mine {
            if mine.block == pos && game.res.facts().is_air(state_id) {
                mine.server_air = true;
            }
        }
    });
}
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if matches!(packet,C::Login(_)|C::Respawn(_)|C::Disconnect(_)){reset();return;}
    if !state(|s| s.session.as_ref().is_some_and(|old| same(game, old))) {
        return;
    }
    match packet {
        C::BlockUpdate(p) => {
            if let Ok(id) = u32::try_from(p.state) {
                observed(game, p.pos, id);
            }
        }
        C::SectionBlocksUpdate(p) => {
            for change in p.changes.iter().take(4096) {
                if let Some((pos, id)) = policy::unpack_section(p.section, *change) {
                    observed(game, pos, id);
                }
            }
        }
        _ => {}
    }
}
// Native multiplayer input otherwise aborts autonomous mining every tick when
// the physical attack button is up. Suppress only that owned idle abort.
pub(super) fn suppress_idle_abort(game: &Game<'_>, down: bool) -> bool {
    !down
        && city_basic(game)
        && hands_free(game)
        && !city_clicks(game)
        && !totem_due(game)
        && city_current()
        && state(|s| {
            s.mine.as_ref().is_some_and(|m| {
                m.waiting.is_none()
                    && super::remote::sunset_city_mining::progress(game, m.block).is_some()
            })
        })
}
pub(super) fn tick(game: &mut Game<'_>, modules: &[Module], tick: u64) {
    let changed = state(|s| s.session.as_ref().is_none_or(|old| !same(game, old)) || tick < s.tick);
    if changed {
        reset();
        state(|s| s.session = session(game));
        // Auto City is a one-shot command, never an enabled-on-reconnect action.
        if modules.iter().any(|m| m.name == CITY && m.on) {
            util::toggle_off(game, CITY);
        }
        return;
    }
    state(|s| s.tick = tick);
    if !city_live(game) {
        cancel(game);
        return;
    }
    let city = modules.iter().find(|m| m.name == CITY && m.on);
    if let Some(m) = city {
        city_tick(game, m, tick);
    } else if state(|s| s.mine.is_some() || s.city_queued.is_some()) {
        stop_city(game, "Disabled", false);
    }
    let trap_enabled=modules.iter().any(|m|m.name==TRAP&&m.on);
    if let Some(m) = modules.iter().find(|m| m.name == TRAP && m.on).filter(|_|city_basic(game)) {
        trap_tick(game, m, tick);
    } else {
        sunset::cancel_turns(TRAP);
        control::release(TRAP);
        state(|s| {
            s.trap_queued = None;
            if !trap_enabled {
                s.trap_empty_warned=false;
                s.trap_plan.clear();s.trap_target=None;s.ledger.clear();s.roof_jumps.clear();
            }
        });
    }
}
pub(super) fn draw(game: &Game<'_>, camera: &sunset::Camera, out: &mut impl Canvas, width: f32) {
    if !sunset::with(|s| s.module(TRAP).is_some_and(|m| m.on && m.b("Show target"))) {
        return;
    }
    let (target, plan) = state(|s| (s.trap_target.clone(), s.trap_plan.clone()));
    if let Some(t) = target {
        super::sunset_visual::draw_box(
            camera,
            out,
            &super::sunset_visual::Look::BOXED,
            t.lo,
            t.hi,
            0xFFFF_A040,
            width,
        );
    }
    for p in plan {
        let lo = p.map(f64::from);
        super::sunset_visual::draw_box(
            camera,
            out,
            &super::sunset_visual::Look::BOXED,
            lo,
            lo.map(|v| v + 1.0),
            0x8080_FFFF,
            width,
        );
    }
    let _ = game;
}
pub(super) fn debug() -> serde_json::Value {
    state(|s| {
        serde_json::json!({"city_block":s.mine.as_ref().map(|m|m.block),
    "city_target":s.mine.as_ref().map(|m|m.target),"city_waiting_server":s.mine.as_ref().is_some_and(|m|m.waiting.is_some()),
    "city_steps":s.mining_steps,"reason":s.reason,"trap_plan":s.trap_plan.len(),"trap_waiting":s.ledger.waiting(),"trap_sent":s.placements})
    })
}
