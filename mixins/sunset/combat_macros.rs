// Original manual explosive transactions. Every sequence originates at a real
// right-click ray; no enemy search, guessed entity id or predicted world edit.
use super::sunset::{self, Category, Module, gui_open, hold, with};
use super::sunset_explosive_damage::{
    Blast, clear_line, local_damage, safety_damage, target_damage,
};
use super::sunset_explosive_policy::{Budget, Limits, in_range, safe};
use super::sunset_macro_policy::{self as policy, Kind, Sequence, Step, Wait};
use super::sunset_util::{self as util, OFFHAND};
use super::{Dim, Game};
use horizon_protocol::{
    connection::Counters,
    packets::{
        Empty,
        play::{self, Serverbound as S},
    },
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::event::MouseButton;

const CRYSTAL: &str = "Crystal Macro";
const ANCHOR: &str = "Anchor Macro";
type Session = Arc<Mutex<Option<Arc<Counters>>>>;

pub(super) fn defaults() -> Vec<Module> {
    let common = |m: Module| {
        m.bool("Protect loot", true)
            .num("Max self damage", 8., 0., 20., 0.5)
            .pick("Damage model", &["Observed equipment", "Unmitigated"])
            .num("Max friend damage", 0., 0., 36., 0.5)
            .num("Min remaining health", 4., 1., 20., 0.5)
    };
    vec![
        common(Module::new(CRYSTAL, Category::Combat, "Manual right-click crystal cycle. Waits for observed base, crystal and removal; checks self and friend safety before exploding. Protect loot pauses module-planned blasts near an observed player death for 30 seconds; it does not track dropped items.")
            .num("Place delay", 0., 0., 10., 1.).num("Break delay", 0., 0., 10., 1.)
            .bool("Auto obsidian", true).num("Obsidian delay", 2., 0., 10., 1.)
            .bool("Switch back",true).bool("Human pace",true)),
        common(Module::new(ANCHOR, Category::Combat, "Manual anchor sequence outside anchor-compatible dimensions. Double refills only a top-face anchor at or above your feet block with replacement supplies. Detonate fallback still checks native self/friend safety. Protect loot pauses planned blasts near an observed player death for 30 seconds, without tracking dropped items.")
            .pick("Mode", &["Single", "Double"]).num("Delay", 0., 0., 5., 1.)
            .bool("Safe anchor", true).pick("If unguarded", &["Detonate", "Cancel"])
            .bool("Switch back",false)),
    ]
}

#[derive(Clone, Copy, Debug)]
struct Click {
    pos: [i32; 3],
    face: i32,
    hit: [f64; 3],
}
#[derive(Clone, Copy, Debug)]
enum Action {
    Use {
        step: Step,
        click: Click,
        slot: usize,
        select: Option<usize>,
        destination: [i32; 3],
        before: u32,
    },
    Attack {
        id: u32,
        at: [f64; 3],
        aim: [f64; 3],
        slot:usize,
    },
}
impl Action {
    fn step(self) -> Step {
        match self {
            Self::Use { step, .. } => step,
            Self::Attack { .. } => Step::CrystalBreak,
        }
    }
    fn aim(self) -> [f64; 3] {
        match self {
            Self::Use { click, .. } => click.hit,
            Self::Attack { aim, .. } => aim,
        }
    }
}
#[derive(Clone)]
struct Run {
    owner: &'static str,
    settings: Arc<Module>,
    sequence: Sequence,
    pos: [i32; 3],
    previous: usize,
    owned: usize,
    session: Session,
    entity: i32,
    dimension: Dim,
    pending: Option<(Action, Option<u64>)>,
    ready: u64,
    crystal: Option<u32>,
    guard: Option<[i32; 3]>,
    token: u64,
    queued_at: Option<Instant>,
    deadline: u64,
    replace_click: Option<Click>,
    guard_block:Option<u32>,
    guard_tries:u8,
    guard_confirmed:bool,
}
struct AnchorPress { tick: u64, session: Session, entity: i32, dimension: Dim, start_allowed: bool }
struct State {
    run: Option<Run>,
    anchor_press: Option<AnchorPress>,
    token: u64,
    anchor_latched: bool,
    retries: policy::Retries,
    last_session: Option<Session>,
    last_deed: Option<u64>,
    pace_gap: u64,
    random:u64,
    main:Option<(usize,Option<horizon_client::items::ItemStack>,u64)>,
}
static STATE: Mutex<State> = Mutex::new(State {
    run: None,
    anchor_press: None,
    token: 0,
    anchor_latched: false,
    retries: policy::Retries::new(),
    last_session: None,
    last_deed:None,pace_gap:0,random:0,main:None,
});
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}
static USER_RIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn user_mouse(game: &Game<'_>, button: MouseButton, pressed: bool) {
    if button == MouseButton::Right {
        let user = !pressed || (super::sunset_combat_control::owner().is_none_or(|o| o == CRYSTAL || o == ANCHOR)
            && game.native_playing() && !gui_open());
        if user { USER_RIGHT.store(pressed, std::sync::atomic::Ordering::Relaxed); }
        // Retain a guard-only quick edge even while inventory/lease conditions
        // pause macro startup. It can consume a protected anchor fallback, but
        // cannot become a delayed macro start after the pause clears.
        if pressed && game.native_playing() && game.native_on(sunset::ID) && !gui_open() {
            if let Some(remote) = game.remote.as_ref() {
                state(|s| s.anchor_press = Some(AnchorPress { tick: game.ticks,
                    session: remote.session.counters.clone(), entity: remote.entity_id,
                    dimension: game.dim, start_allowed: user && allowed(game) }));
            }
        }
    }
}
pub(super) fn focus_lost() {
    USER_RIGHT.store(false, std::sync::atomic::Ordering::Relaxed);
    state(|s| s.anchor_press = None);
}
fn down(game: &Game<'_>) -> bool {
    game.physical_buttons.contains(&MouseButton::Right) || USER_RIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

fn allowed(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && util::in_world(game)
        && !gui_open()
        && !game.creative
        && !game.spectator()
        && game.selected < 9
        && game.using.is_none()
        && game.mining.is_none()
        && !game.left
        && !super::sunset_player::gap_eating()
        && util::inventory_idle(game)
        && !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft)
        && !game.keys.contains(&winit::keyboard::KeyCode::ShiftRight)
        && !with(|s| ["Blink", "Auto EXP"].iter().any(|n| s.is_on(n)))
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.)
        && game.mobs.len() <= 256
        && game.crystals.len() <= 256
}
fn same_session(game: &Game<'_>, run: &Run) -> bool {
    game.dim == run.dimension
        && game.remote.as_ref().is_some_and(|r| {
            r.entity_id == run.entity && Arc::ptr_eq(&r.session.counters, &run.session)
        })
}
fn enabled(run: &Run) -> bool {
    with(|s| {
        s.module(run.owner)
            .is_some_and(|m| m.on && m == run.settings.as_ref())
    })
}

// GUI/death/world transitions cancel the transaction; restoration is allowed
// only while the same session and selected slot are still owned by this run.
pub(super) fn reset(game: &mut Game<'_>) {
    let run = state(|s| {
        s.token = s.token.wrapping_add(1);
        s.anchor_press = None;
        s.run.take()
    });
    sunset::cancel_turns(CRYSTAL);
    sunset::cancel_turns(ANCHOR);
    if let Some(run) = run {
        if super::sunset_combat_control::owns(run.owner)
            && same_session(game, &run)
            && game.using.is_none()
            && !super::sunset_player::gap_eating()
        {
            if let Some(previous) = policy::restore(
                run.previous,
                run.owned,
                game.selected,
                run.settings.b("Switch back"),
            ) {
                hold(game, previous);
            }
        }
        super::sunset_combat_control::release(run.owner);
    }
}
pub(super) fn active() -> bool {
    state(|s| s.run.is_some())
}

fn stack(game: &Game<'_>, slot: usize, name: &str) -> bool {
    let stack = if slot == OFFHAND {
        game.offhand.as_ref()
    } else {
        game.inventory.get(slot).and_then(|v| v.as_ref())
    };
    stack.is_some_and(|s| s.count > 0 && util::short(&s.item) == name)
}
fn item_slot(game: &Game<'_>, name: &str) -> Option<usize> {
    util::find_held(game, |n| n == name).filter(|&slot| stack(game, slot, name))
}

// Unknown runtime components cannot establish that main-hand use passes.
// Native item prototypes provide the ordinary component facts. Cosmetic and
// durability wire additions do not alter use; removals are conservatively unknown.
fn known_click_patch(stack:&horizon_client::items::ItemStack)->bool {
    if stack.extra.as_ref().is_some_and(|extra|!extra.other.is_empty()){return false;}
    let Some(patch)=stack.wire_patch() else{return true;};
    use horizon_protocol::{buf::Reader,item::component as c};
    let mut reader=Reader::new(&patch);
    let Ok(added)=reader.count(64) else{return false;};
    if reader.count(64).ok()!=Some(0){return false;}
    for _ in 0..added {
        let Ok(kind)=reader.varint() else{return false;};
        if !matches!(kind,c::DAMAGE|c::ENCHANTMENTS|c::CUSTOM_NAME|c::ITEM_NAME
            |c::LORE|c::REPAIR_COST|c::ITEM_MODEL|c::CUSTOM_MODEL_DATA
            |c::TOOLTIP_DISPLAY|c::ENCHANTMENT_GLINT_OVERRIDE){return false;}
        let Ok(length)=reader.count(reader.remaining()) else{return false;};
        if reader.bytes(length).is_err(){return false;}
    }
    reader.remaining()==0
}
fn click_facts(game:&Game<'_>,slot:usize)->policy::ClickFacts {
    let Some(stack)=game.inventory.get(slot).and_then(|stack|stack.as_ref()).filter(|s|s.count>0)
        else{return policy::ClickFacts{empty:true,..Default::default()};};
    let Some(info)=game.items().get(&stack.item) else{return policy::ClickFacts::default();};
    let has=|name:&str|info.components.iter().any(|component|component==name);
    let tag=|name:&str|game.items().item_tags.get(name).is_some_and(|items|items.contains(&*stack.item));
    policy::ClickFacts{empty:false,known:known_click_patch(stack),
        consuming:has("minecraft:consumable"),blocking:has("minecraft:blocks_attacks"),
        kinetic:has("minecraft:kinetic_weapon"),equip_swap:info.equip_slot.is_some()&&info.swappable,
        sword:tag("minecraft:swords"),axe:tag("minecraft:axes"),
        mace:util::short(&stack.item)=="mace",totem:util::short(&stack.item)=="totem_of_undying"}
}
fn crystal_route(game:&Game<'_>,name:&str,base:bool,preferred:usize)->Option<policy::UseRoute> {
    policy::placement_route(game.selected,preferred,std::array::from_fn(|slot|stack(game,slot,name)),
        std::array::from_fn(|slot|policy::click_through(click_facts(game,slot),base)),stack(game,OFFHAND,name))
}
fn crystal_use_action(game:&Game<'_>,run:&Run,step:Step,click:Click,name:&str,
    destination:[i32;3])->Option<Action> {
    let base=matches!(util::short(&util::facts(game,click.pos).name),"obsidian"|"bedrock");
    let route=crystal_route(game,name,base,run.previous)?;
    let selected=route.select.unwrap_or(game.selected);
    Some(Action::Use{step,click,select:route.select,slot:if route.offhand{OFFHAND}else{selected},
        destination,before:game.world.block(destination[0],destination[1],destination[2]).0})
}

fn anchor_charge(game: &Game<'_>, p: [i32; 3]) -> Option<u8> {
    if !game.world.loaded(p[0], p[2]) {
        return None;
    }
    let b = game.world.block(p[0], p[1], p[2]);
    if !game.is_anchor(b) {
        return None;
    }
    game.pack
        .assets
        .registry
        .state(b)?
        .properties
        .get("charges")?
        .parse::<u8>()
        .ok()
        .filter(|n| *n <= 4)
}
fn center(p: [i32; 3], crystal: bool) -> [f64; 3] {
    [
        f64::from(p[0]) + 0.5,
        f64::from(p[1]) + if crystal { 1. } else { 0.5 },
        f64::from(p[2]) + 0.5,
    ]
}
fn name_is(game: &Game<'_>, p: [i32; 3], name: &str) -> bool {
    util::short(&util::facts(game, p).name) == name
}
fn inert(game: &Game<'_>, p: [i32; 3]) -> bool {
    game.world.loaded(p[0], p[2])
        && util::facts(game, p).full_collision
        && matches!(
            util::short(&util::facts(game, p).name),
            "stone"
                | "cobblestone"
                | "obsidian"
                | "crying_obsidian"
                | "bedrock"
                | "deepslate"
                | "cobbled_deepslate"
                | "dirt"
                | "grass_block"
                | "netherrack"
                | "end_stone"
                | "netherite_block"
        )
}
fn visible(game: &Game<'_>, click: Click, budget: &mut Budget) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let Some(end) = policy::outside(click.hit, click.face) else {
        return false;
    };
    game.world.loaded(click.pos[0], click.pos[2])
        && in_range(eye, click.hit, 4.5)
        && clear_line(game, eye, end, None, budget)
}
fn support(game: &Game<'_>, p: [i32; 3], budget: &mut Budget) -> Option<Click> {
    if !game.world.loaded(p[0], p[2]) || !util::can_place(game, p, true) {
        return None;
    }
    for (d, face) in [
        ([0, -1, 0], 1),
        ([0, 1, 0], 0),
        ([0, 0, -1], 3),
        ([0, 0, 1], 2),
        ([-1, 0, 0], 5),
        ([1, 0, 0], 4),
    ] {
        let n = std::array::from_fn(|i| p[i] + d[i]);
        if !inert(game, n) {
            continue;
        }
        let hit = std::array::from_fn(|i| f64::from(n[i]) + 0.5 - f64::from(d[i]) * 0.5);
        let click = Click { pos: n, face, hit };
        if visible(game, click, budget) {
            return Some(click);
        }
    }
    None
}
fn anchor_click(game: &Game<'_>, p: [i32; 3], budget: &mut Budget) -> Option<Click> {
    let eye = game.eye_position()?;
    let c = center(p, false);
    let axis = (0..3).max_by(|&a, &b| (eye[a] - c[a]).abs().total_cmp(&(eye[b] - c[b]).abs()))?;
    let positive = eye[axis] >= c[axis];
    let mut hit = c;
    hit[axis] += if positive { 0.5 } else { -0.5 };
    let face = match (axis, positive) {
        (0, false) => 4,
        (0, true) => 5,
        (1, false) => 0,
        (1, true) => 1,
        (2, false) => 2,
        _ => 3,
    };
    let click = Click { pos: p, face, hit };
    visible(game, click, budget).then_some(click)
}
fn crystal_room(game: &Game<'_>, p: [i32; 3]) -> bool {crystal_space(game,p,true)}
fn crystal_space(game: &Game<'_>, p: [i32; 3], require_base:bool) -> bool {
    if p[1] < -64
        || p[1] >= 318
        || !game.world.loaded(p[0], p[2])
        || (require_base && !matches!(
            util::short(&util::facts(game, p).name),
            "obsidian" | "bedrock"
        ))
        || !util::is_air(game, [p[0], p[1] + 1, p[2]])
    {
        return false;
    }
    let c = center(p, true);
    let hits = |b: horizon_core::geometry::Aabb| {
        let (a, z) = (b.min_position(), b.max_position());
        a.x < c[0] + 0.5
            && z.x > c[0] - 0.5
            && a.y < c[1] + 2.
            && z.y > c[1]
            && a.z < c[2] + 0.5
            && z.z > c[2] - 0.5
    };
    !game.player.as_ref().is_some_and(|p| hits(p.bounds()))
        && !game.mobs.iter().any(|m| !m.removed && hits(m.bounds()))
        && !game.crystals.iter().any(|e| {
            (e.position.x - c[0]).abs() < 1.5
                && (e.position.z - c[2]).abs() < 1.5
                && e.position.y < c[1] + 2.
                && e.position.y + 2. > c[1]
        })
}
fn crystal_on(game: &Game<'_>, p: [i32; 3]) -> Option<u32> {
    let c = center(p, true);
    game.crystals
        .iter()
        .find(|e| {
            e.id != 0
                && !e.invulnerable
                && in_range(c, [e.position.x, e.position.y, e.position.z], 0.25)
        })
        .map(|e| e.id)
}
fn direction(game: &Game<'_>) -> [f64; 3] {
    let (y, p) = (
        f64::from(game.yaw).to_radians(),
        f64::from(game.pitch).to_radians(),
    );
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}
fn crosshair_crystal(game: &Game<'_>, budget: &mut Budget) -> Option<(u32, [f64; 3], [f64; 3])> {
    let eye = game.eye_position()?;
    let dir = direction(game);
    let mut best = None;
    for e in &game.crystals {
        if e.id == 0 || e.invulnerable {
            continue;
        }
        let at = [e.position.x, e.position.y, e.position.z];
        let Some(hit) = policy::ray(
            eye,
            dir,
            [at[0] - 1., at[1], at[2] - 1.],
            [at[0] + 1., at[1] + 2., at[2] + 1.],
            3.,
        ) else {
            continue;
        };
        let d = util::dist(eye, hit);
        if best.as_ref().is_some_and(|(old, _, _, _)| *old <= d) {
            continue;
        }
        let end = std::array::from_fn(|i| hit[i] + (eye[i] - hit[i]) * 1.0e-5);
        if clear_line(game, eye, end, None, budget) {
            best = Some((d, e.id, at, hit));
        }
    }
    best.map(|(_, id, at, aim)| (id, at, aim))
}
fn camera_click(game:&Game<'_>)->Option<Click> {
    let hit=game.pick()?;
    Some(Click{pos:hit.pos,face:policy::face(hit.face)?,hit:hit.point})
}
fn crystal_intent(game:&Game<'_>,settings:&Module,preferred:usize,budget:&mut Budget)->Option<([i32;3],Step,Option<u32>)> {
    if let Some((id,at,_))=crosshair_crystal(game,budget){
        return Some(([at[0].floor() as i32,at[1].floor() as i32-1,at[2].floor() as i32],Step::CrystalBreak,Some(id)));
    }
    let click=camera_click(game)?;
    if matches!(util::short(&util::facts(game,click.pos).name),"obsidian"|"bedrock"){
        return (crystal_room(game,click.pos) && crystal_route(game,"end_crystal",true,preferred).is_some()).then_some((click.pos,Step::CrystalPlace,None));
    }
    if !settings.b("Auto obsidian") || !inert(game,click.pos)
        || crystal_route(game,"obsidian",false,preferred).is_none()
        || crystal_route(game,"end_crystal",true,preferred).is_none(){return None;}
    let normal=match click.face{0=>[0,-1,0],1=>[0,1,0],2=>[0,0,-1],3=>[0,0,1],4=>[-1,0,0],5=>[1,0,0],_=>return None};
    let target=std::array::from_fn(|i|click.pos[i]+normal[i]);
    (util::can_place(game,target,true) && crystal_space(game,target,false)
        && visible(game,click,budget)).then_some((target,Step::Base,None))
}
fn random(s:&mut State)->u64 {
    s.random=s.random.wrapping_add(0x9e3779b97f4a7c15);
    let mut n=s.random;n=(n^(n>>30)).wrapping_mul(0xbf58476d1ce4e5b9);
    n=(n^(n>>27)).wrapping_mul(0x94d049bb133111eb);n^(n>>31)
}
fn delay_for(run:&Run,step:Step)->f64 {
    match step{Step::Base=>run.settings.n("Obsidian delay"),Step::CrystalPlace=>run.settings.n("Place delay"),
        Step::CrystalBreak=>run.settings.n("Break delay"),_=>run.settings.n("Delay")}
}
pub(super) fn note_main(game:&Game<'_>) {
    let current=&game.inventory[game.selected];
    state(|s|{
        if s.main.as_ref().is_none_or(|(slot,stack,tick)|*slot!=game.selected || stack!=current || game.ticks<*tick){
            s.main=Some((game.selected,current.clone(),game.ticks));
        }
    });
}
fn enchant_damage(game:&Game<'_>,stack:&horizon_client::items::ItemStack)->bool {
    let mut sum=0.0f32;
    for (id,level) in stack.enchantments(){
        let Some(enchantment)=game.items().enchantments.get(id) else{return false;};
        for effect in enchantment.effects["minecraft:damage"].as_array().into_iter().flatten(){
            if effect.get("requirements").is_some_and(|v|!v.is_null())
                || effect["effect"]["type"]!="minecraft:add"{return false;}
            sum+=horizon_client::enchant::level_value(&effect["effect"]["value"],*level);
        }
    }
    sum.is_finite() && sum>0.0
}
pub(super) fn weapon_route(game:&Game<'_>)->Option<usize> {
    let bare=1.0+game.effect_attack_bonus();let mut fallback=None;
    for slot in std::iter::once(game.selected).chain((0..9).filter(|slot|*slot!=game.selected)){
        let Some(stack)=game.inventory[slot].as_ref() else {
            if bare>0.0 {if slot==game.selected{return Some(slot);}fallback.get_or_insert(slot);}continue;
        };
        let Some(info)=game.items().get(&stack.item) else{continue;};
        if stack.count==0 || horizon_client::spear::Kinetic::of(&stack.item).is_some()
            || game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(stack,info.use_cooldown.as_ref())){continue;}
        if enchant_damage(game,stack){return Some(slot);}
        if (bare+info.attack_damage).is_finite() && bare+info.attack_damage>0.0 {
            if slot==game.selected{return Some(slot);}fallback.get_or_insert(slot);
        }
    }
    fallback
}
pub(super) fn weak_settling(game:&Game<'_>)->bool {
    let bare=1.0+game.effect_attack_bonus();
    bare<=0.0 && !game.inventory[game.selected].as_ref().is_some_and(|s|enchant_damage(game,s))
        && state(|s|s.main.as_ref().is_none_or(|(_,_,since)|game.ticks<*since || game.ticks-*since<2))
}
fn planned_blast(kind: Kind, pos: [i32; 3]) -> Blast {
    match kind {
        Kind::Anchor => Blast::anchor(pos),
        Kind::Crystal => Blast::explosion([f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.0, f64::from(pos[2]) + 0.5], 6.0),
    }
}
fn loot_allowed(game: &Game<'_>, run: &Run) -> bool {
    super::sunset_loot_guard::allows(game, planned_blast(run.sequence.kind, run.pos), run.settings.b("Protect loot"))
}
fn safe_blast(game: &mut Game<'_>, settings: &Module, blast: Blast, budget: &mut Budget) -> bool {
    if !super::sunset_loot_guard::allows(game, blast, settings.b("Protect loot")) { return false; }
    let Some(own) =
        local_damage(game, blast, budget).map(|p| safety_damage(p, settings.p("Damage model")))
    else {
        return false;
    };
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let health = f64::from(p.health.health() + p.health.absorption());
    let names = with(|s| s.friends.clone());
    let own_id = sunset::own_id(game);
    let mut friends = Vec::new();
    for m in &game.mobs {
        if m.removed || !m.alive() || m.kind.id != "minecraft:player" || Some(m.id) == own_id {
            continue;
        }
        let Some(name) = game.remote_player_name(m.id) else {
            return false;
        };
        let t = util::mob_target(m);
        if !t
            .pos
            .iter()
            .chain(t.lo.iter())
            .chain(t.hi.iter())
            .all(|n| n.is_finite())
        {
            return false;
        }
        if names.iter().any(|f| f.eq_ignore_ascii_case(name)) && in_range(blast.center, t.pos, 12.)
        {
            friends.push(t);
        }
    }
    let mut friend: f64 = 0.;
    for t in friends {
        let Some(p) = target_damage(game, &t, blast, budget) else {
            return false;
        };
        friend = friend.max(p.upper);
    }
    safe(
        Limits {
            min_target: 0.,
            max_self: settings.n("Max self damage"),
            max_friend: settings.n("Max friend damage"),
            min_remaining: settings.n("Min remaining health"),
            anti_suicide: true,
        },
        0.,
        own,
        health,
        friend,
    )
}

fn body_points(game:&Game<'_>)->Option<Vec<[f64;3]>> {
    let b=game.player.as_ref()?.bounds();let(lo,hi)=(b.min_position(),b.max_position());
    super::sunset_explosive_policy::samples(lo.components(),hi.components())
}
fn guard_open_points(game:&Game<'_>,anchor:[i32;3],budget:&mut Budget)->Option<(usize,Vec<[f64;3]>)> {
    let points=body_points(game)?;let total=points.len();let blast=center(anchor,false);
    let mut open=Vec::with_capacity(total);
    for point in points {
        match super::sunset_explosive_damage::known_line(game,point,blast,Some(anchor),budget)? {
            true=>open.push(point),false=>{},
        }
    }
    Some((total,open))
}
fn covered(game:&Game<'_>,anchor:[i32;3],budget:&mut Budget)->bool {
    guard_open_points(game,anchor,budget)
        .is_some_and(|(total,open)|policy::guard_already_covered(total,open.len()))
}
fn guard_slot(game:&Game<'_>,run:&Run)->Option<usize> {
    guard_slot_at(game,run.pos,run.settings.p("Mode")=="Double")
}
fn guard_slot_at(game:&Game<'_>,anchor:[i32;3],double:bool)->Option<usize> {
    let mut weaker=None;
    for slot in 0..9 {
        let Some(stack)=game.inventory[slot].as_ref().filter(|s|s.count>0) else{continue;};
        if util::short(&stack.item)=="glowstone"{continue;}
        let Some(state)=game.items().get(&stack.item).and_then(|i|i.block) else{continue;};
        let facts=game.res.facts().get(state);
        if !facts.full_collision || facts.block_entity
            || matches!(util::short(&facts.name),"tnt"|"respawn_anchor")
            || facts.classes.iter().any(|c|c.ends_with("FallingBlock")){continue;}
        if facts.explosion_resistance>=600.0{return Some(slot);}
        weaker.get_or_insert(slot);
    }
    if weaker.is_some(){return weaker;}
    let needed=u32::from(anchor_charge(game,anchor).unwrap_or(0)==0)+u32::from(double);
    let count=game.inventory[..9].iter().flatten().filter(|s|util::short(&s.item)=="glowstone")
        .fold(0u32,|n,s|n.saturating_add(s.count));
    (count>needed).then(||util::find_hotbar(game,|n|n=="glowstone")).flatten()
}
fn guard_place(game:&Game<'_>,anchor:[i32;3],budget:&mut Budget)->Option<([i32;3],Click)> {
    let feet=util::block_pos(game)?;let eye=game.eye_position()?;
    let player=game.player.as_ref()?.bounds();let (_,points)=guard_open_points(game,anchor,budget)?;
    let needed=policy::guard_shade_needed(points.len())?;
    let blast=center(anchor,false);let mut cells=Vec::with_capacity(12);
    for dx in -1..=1 {for dz in -1..=1 {
        if dx!=0 || dz!=0 {cells.push([feet[0]+dx,feet[1],feet[2]+dz]);}
    }}
    for d in [[1,0],[-1,0],[0,1],[0,-1]]{
        let p=[anchor[0]+d[0],anchor[1],anchor[2]+d[1]];
        if !cells.contains(&p){cells.push(p);}
    }
    let mut ranked=Vec::with_capacity(12);
    for cell in cells {
        let lo=cell.map(f64::from);let hi=lo.map(|n|n+1.0);
        let box_=horizon_core::geometry::Aabb::new(
            horizon_core::geometry::Vec3::new(lo[0],lo[1],lo[2]),
            horizon_core::geometry::Vec3::new(hi[0],hi[1],hi[2]));
        if cell==anchor || box_.intersects(player)
            || policy::ray(eye,std::array::from_fn(|i|blast[i]-eye[i]),lo,hi,util::dist(eye,blast)).is_some(){continue;}
        let shade=points.iter().filter(|point|policy::ray(**point,
            std::array::from_fn(|i|blast[i]-point[i]),lo,hi,util::dist(**point,blast)).is_some()).count();
        if shade<needed {continue;}
        ranked.push((cell,shade,util::dist(center(cell,false),blast)));
    }
    ranked.sort_by(|a,b|b.1.cmp(&a.1).then(a.2.total_cmp(&b.2)));
    for(cell,_,_)in ranked {if let Some(click)=support(game,cell,budget){return Some((cell,click));}}
    None
}
fn use_action(
    game: &Game<'_>,
    step: Step,
    click: Click,
    slot: usize,
    destination: [i32; 3],
) -> Action {
    Action::Use {
        step,
        click,
        slot,
        select: None,
        destination,
        before: game
            .world
            .block(destination[0], destination[1], destination[2])
            .0,
    }
}
fn detonator(game: &Game<'_>) -> Option<usize> {
    // Partial charged anchors pass to offhand glowstone; never accidentally
    // charge during a transaction that was approved as a detonation.
    if stack(game, OFFHAND, "glowstone") {
        return None;
    }
    util::find_hotbar(game,|n|n=="totem_of_undying").or_else(||(0..9)
        .find(|&slot|combat_item(game,game.inventory[slot].as_ref())))
}
fn melee_item(game:&Game<'_>,stack:&horizon_client::items::ItemStack)->bool {
    util::short(&stack.item)=="mace" || ["minecraft:swords","minecraft:axes"].iter().any(|tag|
        game.items().item_tags.get(*tag).is_some_and(|items|items.contains(&*stack.item)))
}
fn combat_item(game:&Game<'_>,stack:Option<&horizon_client::items::ItemStack>)->bool {
    stack.is_none_or(|stack|stack.count==0 || util::short(&stack.item)=="totem_of_undying" || melee_item(game,stack))
}
fn recognized_hand(game: &Game<'_>, kind: Kind) -> bool {
    let held = game.inventory.get(game.selected).and_then(|s| s.as_ref());
    match kind {
        Kind::Crystal => {
            stack(game, game.selected, "end_crystal")
                || (stack(game,OFFHAND,"end_crystal") && combat_item(game,held))
                || (held.is_some_and(|s|melee_item(game,s))
                    && (game.offhand.is_none() || stack(game,OFFHAND,"totem_of_undying"))
                    && util::find_hotbar(game,|n|n=="end_crystal").is_some())
        }
        Kind::Anchor => true, // Actual ray intent and detonator preflight decide.
    }
}

// Called immediately before vanilla's native use-item operation. True consumes
// that one operation, including repeats during an owned manual sequence.
pub(super) fn intercept_use(game: &mut Game<'_>) -> bool {
    // Native use clicks are consumed on an input tick, after a quick release
    // may already have cleared held state. Retain only one fresh press in the
    // same remote session; consuming it here never starts a held Crystal run.
    let press = state(|s| s.anchor_press.take()).filter(|press| {
        game.ticks >= press.tick && game.ticks - press.tick <= 2
            && game.dim == press.dimension
            && game.remote.as_ref().is_some_and(|r| r.entity_id == press.entity
                && Arc::ptr_eq(&r.session.counters, &press.session))
    });
    let held = down(game);
    if !held && press.is_none() { return false; }
    if let Some(kind) = state(|s| s.run.as_ref().map(|r| r.sequence.kind)) {
        return held || kind == Kind::Anchor;
    }
    let anchor = with(|s| s.module(ANCHOR).filter(|m| m.on).cloned());
    if state(|s| s.anchor_latched) && anchor.is_some() {
        return true;
    }
    let pick = game.pick().map(|hit| (hit.pos, hit.face));
    if game.native_window() && game.native_on(sunset::ID) && util::in_world(game) && !game.anchor_works() {
        if let (Some(m), Some((p, _))) = (anchor.as_ref(), pick) {
            if anchor_charge(game, p).is_some()
                && !super::sunset_loot_guard::allows(game, Blast::anchor(p), m.b("Protect loot")) {
                return true;
            }
        }
    }
    if !held && !press.is_some_and(|p| p.start_allowed) { return false; }
    if !allowed(game) {
        return false;
    }
    let crystal = with(|s| s.module(CRYSTAL).filter(|m| m.on).cloned());
    let mut budget = Budget::new(16);
    let on_anchor = pick.is_some_and(|(p, _)| anchor_charge(game, p).is_some());
    let anchor_intent = on_anchor || stack(game, game.selected, "respawn_anchor");
    if !held && !anchor_intent { return false; }
    let (settings, kind, pos, step, cid) = if anchor_intent {
        let Some(m) = anchor else { return false; };
        if game.anchor_works() { return false; }
        let Some((p, face)) = pick else {
            return false;
        };
        let target = if on_anchor {
            p
        } else {
            if !inert(game, p) || policy::face(face).is_none() {
                return false;
            }
            std::array::from_fn(|i| p[i] + face[i])
        };
        if !on_anchor && support(game, target, &mut budget).is_none() {
            return false;
        }
        if !super::sunset_loot_guard::allows(game, Blast::anchor(target), m.b("Protect loot")) { return true; }
        if !recognized_hand(game, Kind::Anchor) || stack(game, OFFHAND, "glowstone") { return false; }
        if !policy::anchor_supplied(anchor_charge(game,target).is_some_and(|charge|charge>0),
            util::find_hotbar(game,|name|name=="glowstone").is_some(),detonator(game).is_some()) {
            return false;
        }
        if m.b("Safe anchor") && m.p("If unguarded")=="Cancel"
            && guard_slot_at(game,target,m.p("Mode")=="Double").is_none(){return false;}
        (
            m,
            Kind::Anchor,
            target,
            if on_anchor {
                Step::Guard
            } else {
                Step::AnchorPlace
            },
            None,
        )
    } else {
        let Some(m) = crystal.filter(|_| recognized_hand(game, Kind::Crystal)) else {
            return false;
        };
        if let Some((cid, at, _)) = crosshair_crystal(game, &mut budget) {
            let p = [
                at[0].floor() as i32,
                at[1].floor() as i32 - 1,
                at[2].floor() as i32,
            ];
            (m, Kind::Crystal, p, Step::CrystalBreak, Some(cid))
        } else {
            let Some((p, face)) = pick else {
                return false;
            };
            if matches!(
                util::short(&util::facts(game, p).name),
                "obsidian" | "bedrock"
            ) {
                if !crystal_room(game, p) {
                    return false;
                }
                (m, Kind::Crystal, p, Step::CrystalPlace, None)
            } else {
                if !m.b("Auto obsidian") || !inert(game, p) || item_slot(game, "obsidian").is_none()
                {
                    return false;
                }
                let target = std::array::from_fn(|i| p[i] + face[i]);
                if support(game, target, &mut budget).is_none() {
                    return false;
                }
                (m, Kind::Crystal, target, Step::Base, None)
            }
        }
    };
    if kind==Kind::Crystal && step!=Step::CrystalBreak {
        let Some(click)=camera_click(game) else{return false;};
        let base=matches!(util::short(&util::facts(game,click.pos).name),"obsidian"|"bedrock");
        let item=if step==Step::Base{"obsidian"}else{"end_crystal"};
        if crystal_route(game,item,base,game.selected).is_none()
            || (step==Step::Base && crystal_route(game,"end_crystal",true,game.selected).is_none()) {return false;}
    }
    if !super::sunset_loot_guard::allows(game, planned_blast(kind, pos), settings.b("Protect loot")) { return true; }
    let owner = if kind == Kind::Anchor {
        ANCHOR
    } else {
        CRYSTAL
    };
    if !super::sunset_combat_control::claim(owner) {
        return false;
    }
    let Some(remote) = game.remote.as_ref() else {
        super::sunset_combat_control::release(owner);
        return false;
    };
    state(|s| {
        if s.last_session
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &remote.session.counters))
        {
            s.retries.clear();
            s.last_deed=None;s.pace_gap=0;s.main=None;
            s.last_session = Some(remote.session.counters.clone());
        }
        s.token = s.token.wrapping_add(1);
        if kind == Kind::Anchor {
            s.anchor_latched = true;
        }
        s.run = Some(Run {
            owner,
            sequence: Sequence::new(kind, step, settings.p("Mode") == "Double"),
            settings: Arc::new(settings),
            pos,
            previous: game.selected,
            owned: game.selected,
            session: remote.session.counters.clone(),
            entity: remote.entity_id,
            dimension: game.dim,
            pending: None,
            ready: game.ticks,
            crystal: cid,
            guard: None,
            token: s.token,
            queued_at: None,
            deadline: game.ticks.saturating_add(if kind==Kind::Anchor{5}else{60}),
            replace_click:None,
            guard_block:None,guard_tries:0,guard_confirmed:false,
        });
    });
    // Ownership starts before the intercepted vanilla packet could be sent.
    true
}

fn observed(game: &Game<'_>, action: Action, run:&Run) -> bool {
    match action {
        Action::Attack { id, .. } => !game.crystals.iter().any(|c| c.id == id),
        Action::Use {
            step,
            destination,
            before,
            ..
        } => match step {
            Step::Base => name_is(game, destination, "obsidian"),
            Step::Guard => run.guard_block.is_some_and(|expected|
                game.res.facts().get(horizon_world::registry::BlockStateId(expected)).name
                    ==util::facts(game,destination).name),
            Step::CrystalPlace => crystal_on(game, destination).is_some(),
            Step::AnchorPlace => anchor_charge(game, destination).is_some(),
            Step::Charge => {
                game.world
                    .block(destination[0], destination[1], destination[2])
                    .0
                    != before
                    && anchor_charge(game, destination).is_some_and(|n| n > 0)
            }
            Step::Detonate => !game.is_anchor(game.world.block(
                destination[0],
                destination[1],
                destination[2],
            )),
            _ => false,
        },
    }
}
fn next_action(game: &mut Game<'_>, run: &Run, budget: &mut Budget) -> Option<Action> {
    let p = run.pos;
    let step = run.sequence.step;
    match step {
        Step::Base | Step::Guard => {
            let destination = if step == Step::Guard { run.guard? } else { p };
            let click=if step==Step::Base {
                let click=camera_click(game)?;
                if crystal_intent(game,&run.settings,run.previous,budget)?.0!=p || !visible(game,click,budget){return None;}
                let normal=match click.face{0=>[0,-1,0],1=>[0,1,0],2=>[0,0,-1],3=>[0,0,1],4=>[-1,0,0],5=>[1,0,0],_=>return None};
                let route=crystal_route(game,"obsidian",false,run.previous)?;
                let slot=if route.offhand{OFFHAND}else{route.select.unwrap_or(game.selected)};
                let native=super::remote::sunset_aura_base::BaseClick{clicked:click.pos,normal,point:click.hit};
                let range=game.reach();let mut rays=12;
                if !super::remote::sunset_aura_base::valid(game,p,slot,native,range,&mut rays){return None;}
                click
            }else{support(game,destination,budget)?};
            if step==Step::Base {crystal_use_action(game,run,step,click,"obsidian",destination)}
            else{Some(use_action(game,step,click,guard_slot(game,run)?,destination))}
        }
        Step::CrystalPlace => {
            if !crystal_room(game, p) {
                return None;
            }
            let click=camera_click(game)?;
            if click.pos!=p{return None;}
            if !visible(game, click, budget)
                || !safe_blast(
                    game,
                    &run.settings,
                    Blast::explosion(center(p, true), 6.0),
                    budget,
                )
            {
                return None;
            }
            crystal_use_action(game,run,step,click,"end_crystal",p)
        }
        Step::CrystalBreak => {
            let id = run.crystal.or_else(|| crystal_on(game, p))?;
            if !state(|s| s.retries.allows(id, game.ticks)) {
                return None;
            }
            // The manual crystal must remain on the real camera ray. Silent
            // automation never searches for a different nearby crystal.
            let (hit_id, at, aim) = crosshair_crystal(game, budget)?;
            if hit_id != id
                || !safe_blast(
                    game,
                    &run.settings,
                    Blast::explosion(at, 6.0),
                    budget,
                )
            {
                return None;
            }
            let slot=weapon_route(game)?;
            if slot==game.selected && weak_settling(game){return None;}
            Some(Action::Attack { id, at, aim,slot })
        }
        Step::AnchorPlace => {
            if game.anchor_works() {
                return None;
            }
            let click=if let Some(click)=run.replace_click.filter(|_|run.sequence.refilled){
                let eye=game.eye_position()?;
                let (yaw,pitch)=super::sunset_combat_wire::sent_look(game)?;
                let (yaw,pitch)=(f64::from(yaw).to_radians(),f64::from(pitch).to_radians());
                let direction=[-yaw.sin()*pitch.cos(),-pitch.sin(),yaw.cos()*pitch.cos()];
                let lo=p.map(f64::from);let hi=lo.map(|n|n+1.0);
                policy::ray(eye,direction,lo,hi,game.reach())?;
                if !visible(game,click,budget){return None;}click
            }else{support(game,p,budget)?};
            Some(use_action(
                game,
                step,
                click,
                item_slot(game, "respawn_anchor")?,
                p,
            ))
        }
        Step::Charge => {
            if game.anchor_works() || anchor_charge(game, p).is_none_or(|n| n >= 4) {
                return None;
            }
            Some(use_action(
                game,
                step,
                anchor_click(game, p, budget)?,
                item_slot(game, "glowstone")?,
                p,
            ))
        }
        Step::Detonate => {
            if game.anchor_works() || anchor_charge(game, p).is_none_or(|n| n == 0) {
                return None;
            }
            if run.settings.b("Safe anchor")
                && run.settings.p("If unguarded") == "Cancel"
                && !run.guard_confirmed && !covered(game, p, budget)
            {
                return None;
            }
            if !safe_blast(
                game,
                &run.settings,
                Blast::anchor(p),
                budget,
            ) {
                return None;
            }
            Some(use_action(
                game,
                step,
                anchor_click(game, p, budget)?,
                if run.sequence.remaining>1{util::find_hotbar(game,|n|n=="respawn_anchor")?}else{detonator(game)?},
                p,
            ))
        }
        Step::Done => None,
    }
}

pub(super) fn tick(game: &mut Game<'_>, modules: &[Module], _ticks: u64) {
    if !active() && !modules.iter().any(|m|m.on && (m.name==CRYSTAL || m.name==ANCHOR)){return;}
    if game.selected<9 {note_main(game);}
    if !game.native_playing() || gui_open() { focus_lost(); }
    if game.ticks % 20 == 0 {
        state(|s| {
            s.retries
                .retain(|id| game.crystals.iter().any(|c| c.id == id))
        });
    }
    if !down(game) {
        state(|s| s.anchor_latched = false);
    }
    let Some(mut run) = state(|s| s.run.clone()) else {
        return;
    };
    if !allowed(game)
        || !same_session(game, &run)
        || !enabled(&run)
        || !loot_allowed(game, &run)
        || game.selected != run.owned
        || game.ticks >= run.deadline
        || (run.sequence.kind == Kind::Crystal && !down(game))
        || !super::sunset_combat_control::claim(run.owner)
    {
        reset(game);
        return;
    }
    if let Some((action, sent)) = run.pending {
        let Some(sent) = sent else {
            if run
                .queued_at
                .is_some_and(|t| t.elapsed() > Duration::from_millis(200))
            {
                reset(game);
            }
            return;
        };
        let observation=observed(game,action,&run);
        let timeout=if action.step()==Step::Guard{run.ready.saturating_sub(sent).max(1)}
            else if run.sequence.kind==Kind::Crystal{
                u64::from(super::sunset_prediction::rtt(game).saturating_add(2).clamp(4,20))
            }else{run.deadline.saturating_sub(sent)};
        let wait=policy::wait(sent,game.ticks,observation,timeout);
        match wait {
            Wait::Pending => return,
            Wait::Expired => {
                if action.step()==Step::Guard && run.guard_tries<2 {
                    run.pending=None;run.guard=None;
                    state(|s|s.run=Some(run));return;
                }
                reset(game);
                return;
            }
            Wait::Acknowledged => {
                if action.step()==Step::Guard{run.guard_confirmed=true;}
                if action.step() == Step::CrystalPlace {
                    run.crystal = crystal_on(game, run.pos);
                }
                run.sequence.acknowledged(action.step());
                run.pending = None;
                state(|s| {
                    if s.run.as_ref().is_some_and(|r| r.token == run.token) {
                        s.run = Some(run.clone());
                    }
                });
            }
        }
    }
    if run.sequence.kind==Kind::Crystal {
        let mut budget=Budget::new(8);
        if let Some((pos,step,cid))=crystal_intent(game,&run.settings,run.previous,&mut budget){
            run.pos=pos;run.sequence=Sequence::new(Kind::Crystal,step,false);run.crystal=cid;
        }else{
            run.deadline=game.ticks.saturating_add(60);
            state(|s|s.run=Some(run));return;
        }
        // The held ray is replanned only after any real prior transaction was
        // observed; it never continues a pinned old base after the camera moved.
        run.deadline=game.ticks.saturating_add(60);
        state(|s|s.run=Some(run.clone()));
    }
    if run.sequence.step == Step::Done {
        reset(game);
        return;
    }
    if game.ticks < run.ready {
        return;
    }
    if run.sequence.kind==Kind::Crystal && !state(|s|policy::action_due(s.last_deed,game.ticks,
        delay_for(&run,run.sequence.step),s.pace_gap)){return;}
    let mut budget = Budget::new(24);
    if run.sequence.step == Step::Guard {
        if !run.settings.b("Safe anchor") || covered(game, run.pos, &mut budget) {
            run.sequence.acknowledged(Step::Guard);
        } else if run.guard.is_none() {
            run.guard = guard_place(game, run.pos, &mut budget).map(|(p, _)| p);
            if run.guard.is_none() || guard_slot(game,&run).is_none() {
                if run.settings.p("If unguarded") == "Cancel" {
                    reset(game);
                    return;
                }
                run.sequence.acknowledged(Step::Guard);
            }
        }
    }
    if run.sequence.step == Step::Charge && anchor_charge(game, run.pos).is_some_and(|n| n > 0) {
        run.sequence.acknowledged(Step::Charge);
    }
    if run.sequence.step==Step::Detonate && run.sequence.remaining>1 {
        let eligible=anchor_click(game,run.pos,&mut budget).is_some_and(|click|
            policy::double_eligible(run.settings.p("Mode")=="Double",click.face,run.pos[1],
                game.player.as_ref().map_or(i32::MAX,|p|p.position.y.floor() as i32),
                util::find_hotbar(game,|n|n=="respawn_anchor").is_some(),
                util::find_hotbar(game,|n|n=="glowstone").is_some()));
        if !eligible{run.sequence.remaining=1;}
    }
    state(|s| {
        if s.run.as_ref().is_some_and(|r| r.token == run.token) {
            s.run = Some(run.clone());
        }
    });
    let Some(action) = next_action(game, &run, &mut budget) else {
        if run.sequence.kind==Kind::Crystal{return;}
        reset(game);
        return;
    };
    if run.sequence.kind==Kind::Crystal && !state(|s|policy::action_due(s.last_deed,game.ticks,
        delay_for(&run,action.step()),s.pace_gap)){return;}
    queue(game, &run, action);
}

fn queue(game: &mut Game<'_>, run: &Run, action: Action) {
    let Some(eye) = game.eye_position() else {
        reset(game);
        return;
    };
    let token = run.token;
    let owner = run.owner;
    let queued = Instant::now();
    state(|s| {
        if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
            r.pending = Some((action, None));
            r.queued_at = Some(queued);
        }
    });
    let (yaw, pitch) = sunset::look_at(eye, action.aim());
    let accepted = sunset::try_turn_then(owner, yaw, pitch, move |game| {
        let Some(run) = state(|s| s.run.clone())
            .filter(|r| r.token == token && r.pending.is_some_and(|(_, sent)| sent.is_none()))
        else {
            return;
        };
        if queued.elapsed() > Duration::from_millis(200)
            || !allowed(game)
            || !same_session(game, &run)
            || !enabled(&run)
            || !loot_allowed(game, &run)
            || game.selected != run.owned
            || !super::sunset_combat_control::owns(owner)
            || !game
                .eye_position()
                .is_some_and(|now| in_range(now, eye, 0.5))
            || (run.sequence.kind == Kind::Crystal && !down(game))
        {
            reset(game);
            return;
        }
        let mut budget = Budget::new(24);
        let Some(fresh) = next_action(game, &run, &mut budget) else {
            reset(game);
            return;
        };
        // A stale look must never be reused for a changed hand, face or entity.
        let same = match (action, fresh) {
            (Action::Attack { id, at,slot, .. }, Action::Attack { id: b, at: c,slot:d, .. }) => {
                id == b && slot==d && in_range(at, c, 0.01)
            }
            (
                Action::Use {
                    step,
                    click,
                    slot,
                    select,
                    destination,
                    before,
                },
                Action::Use {
                    step: b,
                    click: c,
                    slot: d,
                    select: selected,
                    destination: e,
                    before: f,
                },
            ) => {
                step == b
                    && slot == d
                    && select == selected
                    && destination == e
                    && before == f
                    && click.pos == c.pos
                    && click.face == c.face
                    && click.hit == c.hit
            }
            _ => false,
        };
        if !same {
            reset(game);
            return;
        }
        match action {
            Action::Attack { id,slot, .. } => {
                if slot!=game.selected {
                    hold(game,slot);state(|s|if let Some(r)=s.run.as_mut().filter(|r|r.token==token){r.owned=slot;});
                    note_main(game);
                }
                if weak_settling(game){
                    state(|s|if let Some(r)=s.run.as_mut().filter(|r|r.token==token){
                        r.pending=None;r.queued_at=None;r.ready=game.ticks.saturating_add(1);
                    });return;
                }
                game.send(S::Attack(play::Attack { id: id as i32 }));
                game.reset_attack_strength();
                state(|s| s.retries.sent(id, game.ticks));
            }
            Action::Use { click, slot, select, step, .. } => {
                // Reselecting a pass-through main hand is part of the accepted
                // deed, not planning; preserve the existing session/slot lease.
                if let Some(selected)=select {
                    hold(game,selected);note_main(game);
                    state(|s|if let Some(r)=s.run.as_mut().filter(|r|r.token==token){r.owned=selected;});
                }
                if slot != OFFHAND {
                    hold(game, slot);note_main(game);
                    state(|s| {
                        if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
                            r.owned = slot;
                        }
                    });
                }
                let sequence = util::sequence(game);
                game.send(S::UseItemOn(play::UseItemOn {
                    hand: i32::from(slot == OFFHAND),
                    pos: click.pos,
                    face: click.face,
                    cursor: std::array::from_fn(|i| {
                        (click.hit[i] - f64::from(click.pos[i])) as f32
                    }),
                    inside: false,
                    world_border: false,
                    sequence,
                }));
                state(|s|if let Some(r)=s.run.as_mut().filter(|r|r.token==token){
                    if step==Step::Guard {
                        r.guard_tries=r.guard_tries.saturating_add(1);
                        r.guard_block=game.stack_in(slot).and_then(|stack|game.items().get(&stack.item)).and_then(|i|i.block).map(|id|id.0);
                    }
                    if step==Step::Detonate && r.sequence.remaining>1 {r.replace_click=Some(click);}
                });
            }
        }
        game.swing_arm();
        game.send(S::Punch(Empty));
        let delay=delay_for(&run,action.step());
        state(|s| {
            if run.sequence.kind==Kind::Crystal {
                s.last_deed=Some(game.ticks);
                s.pace_gap=if run.settings.b("Human pace"){policy::human_gap(random(s))}else{0};
            }
            if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
                r.pending = Some((action, Some(game.ticks)));
                if action.step()==Step::Guard && r.settings.p("If unguarded")=="Detonate" {
                    r.pending=None;r.sequence.acknowledged(Step::Guard);
                }
                r.queued_at = None;
                r.ready = game.ticks.saturating_add(if run.sequence.kind==Kind::Crystal{1}else{delay.clamp(0.,5.) as u64+1});
                let window=if run.sequence.kind==Kind::Crystal{60}
                    else if action.step()==Step::Guard && run.settings.p("If unguarded")=="Cancel"{
                        u64::from(super::sunset_prediction::rtt(game).saturating_add(2).clamp(4,20))
                    }else{5};
                r.deadline = r.ready.saturating_add(window);
            }
        });
    });
    if !accepted {
        state(|s| {
            if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
                r.pending = None;
                r.queued_at = None;
            }
        });
    }
}
