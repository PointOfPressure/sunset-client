// Original bounded native adapter for shared explosive planning. No packets.
use super::{Game,Dim};
use super::{sunset_util as util,sunset_prediction as prediction};
use super::sunset_util::Target;
use std::sync::{Arc,Mutex};
type Connection=Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
struct Context {session:Connection,dim:Dim,id:i32,tick:u64,block:f64,entity:f64,focus:Option<u32>}
static CONTEXT:Mutex<Option<Context>>=Mutex::new(None);
fn same(game:&Game<'_>,c:&Context)->bool {game.dim==c.dim && game.remote.as_ref().is_some_and(|r|r.entity_id==c.id&&Arc::ptr_eq(&r.session.counters,&c.session))}
pub(super) fn refresh(game:&mut Game<'_>)->bool {
    let Some(remote)=game.remote.as_ref() else {return false;};
    let (session,id)=(remote.session.counters.clone(),remote.entity_id);
    let Some(player)=game.player.as_mut() else {return false;};
    let Some((block,entity))=player.attribute("minecraft:block_interaction_range").ok()
        .zip(player.attribute("minecraft:entity_interaction_range").ok()) else {return false;};
    if ![block,entity].iter().all(|n|n.is_finite()&&*n>0.1&&*n<=64.0) {return false;}
    let mut guard=CONTEXT.lock().unwrap_or_else(|e|e.into_inner());
    let focus=guard.as_ref().filter(|c|same(game,c)).and_then(|c|c.focus);
    *guard=Some(Context{session,dim:game.dim,id,tick:game.ticks,block,entity,focus});true
}
fn range(game:&Game<'_>,entity:bool)->f64 {
    CONTEXT.lock().unwrap_or_else(|e|e.into_inner()).as_ref().filter(|c|same(game,c)&&c.tick==game.ticks)
        .map_or(0.0,|c|if entity {c.entity}else{c.block})
}
// Exact current interaction attributes for manual cursor picking. Autonomous
// planners retain their own small placement/rotation margins below.
pub(super) fn interaction_ranges(game:&Game<'_>)->Option<(f64,f64)> {
    let guard=CONTEXT.lock().unwrap_or_else(|e|e.into_inner());
    let c=guard.as_ref().filter(|c|same(game,c)&&c.tick==game.ticks)?;
    Some((c.block,c.entity))
}
pub(super) fn block_range(game:&Game<'_>)->f64 {(range(game,false)-0.1).max(0.0)}
pub(super) fn anchor_range(game:&Game<'_>)->f64 {(range(game,false)-0.01).max(0.0)}
pub(super) fn entity_range(game:&Game<'_>)->f64 {(range(game,true)-0.1).max(0.0)}
pub(super) fn target_score(distance:f64,focused:bool,preferred:bool,weak:bool)->f64 {
    distance-if focused {2.0}else{0.0}-if preferred {6.0}else{0.0}-if weak {2.0}else{0.0}
}
pub(super) fn players(game:&Game<'_>)->Option<(Vec<Target>,Vec<Target>)> {
    if game.mobs.len()>512 {return None;}
    let p=game.player.as_ref()?;let feet=[p.position.x,p.position.y,p.position.z];
    let own=game.remote.as_ref()?.entity_id as u32;
    let names=super::sunset::with(|s|s.friends.clone());
    let focus=CONTEXT.lock().unwrap_or_else(|e|e.into_inner()).as_ref().filter(|c|same(game,c)).and_then(|c|c.focus);
    let preferred=super::sunset_crystal_bot::steer(game).map(|s|s.target);
    let (mut targets,mut friends)=(Vec::new(),Vec::new());
    for mob in &game.mobs {
        if mob.kind.id!="minecraft:player" || mob.removed || !mob.alive() || mob.id==own {continue;}
        let t=util::mob_target(mob);
        if !t.pos.iter().chain(t.lo.iter()).chain(t.hi.iter()).all(|v|v.is_finite()) {return None;}
        let friend=game.remote_player_name(mob.id).is_some_and(|name|names.iter().any(|f|f.eq_ignore_ascii_case(name)));
        if friend {friends.push(t);} else if util::dist(feet,t.pos)<10.0 && util::player_enemy(game,mob) {targets.push(t);}
    }
    let score=|t:&Target|target_score(util::dist(feet,t.pos),focus==Some(t.id),preferred==Some(t.id),
        prediction::effective_health(game,t).unwrap_or(f64::from(t.health))<=10.0);
    targets.sort_by(|a,b|score(a).total_cmp(&score(b)));
    targets.truncate(3);
    if let Some(c)=CONTEXT.lock().unwrap_or_else(|e|e.into_inner()).as_mut().filter(|c|same(game,c)) {c.focus=targets.first().map(|t|t.id);}
    Some((targets,friends))
}
pub(super) fn breakable(game:&Game<'_>,base:[i32;3])->bool {
    game.eye_position().is_some_and(|eye|super::sunset_explosive_choice::future_crystal_breakable(eye,base,entity_range(game)+0.1))
}
pub(super) fn endangered(damage:f64,health:f64,creative:bool)->bool {
    !damage.is_finite() || !health.is_finite() || damage<0.0 || health<=0.0
        || !creative && damage*1.15+2.0>=health
}
pub(super) fn friend_health(game:&Game<'_>,t:&Target)->f64 {
    prediction::effective_health(game,t).unwrap_or(f64::from(t.health))
}
pub(super) fn fixed_look(game:&Game<'_>)->bool {
    game.riding.is_some() || game.player.as_ref().is_some_and(|p|p.fall_flying||p.auto_spin_attack_ticks>0||p.swimming)
        || super::sunset_render::camera_takes_mouse()
}
pub(super) fn next_look(game:&Game<'_>,desired:(f32,f32),speed:f64)->(f32,f32) {
    if fixed_look(game) {(game.yaw,game.pitch)} else {super::sunset_aura::next_look(game,desired,speed)}
}
pub(super) fn same_camera(game:&Game<'_>)->bool {
    super::sunset_combat_wire::sent_look(game).is_some_and(|(yaw,pitch)|
        ((yaw-game.yaw+180.0).rem_euclid(360.0)-180.0).abs()<=2.0&&(pitch-game.pitch).abs()<=2.0)
}
pub(super) fn aim_eye(game:&Game<'_>)->Option<[f64;3]> {
    let eye=game.eye_position()?;
    if fixed_look(game){return Some(eye);}
    let p=game.player.as_ref()?;let velocity=[p.velocity.x,p.velocity.y,p.velocity.z];
    velocity.iter().all(|v|v.is_finite()).then(||std::array::from_fn(|i|eye[i]+velocity[i]))
}
pub(super) fn may_take_hand(game:&Game<'_>,anchors:bool)->bool {
    let main=game.inventory.get(game.selected).and_then(|s|s.as_ref());
    combat_item(game,main)||main.is_some_and(|s|matches!(util::short(&s.item),"end_crystal"|"obsidian")
        ||anchors&&matches!(util::short(&s.item),"respawn_anchor"|"glowstone"))
}
pub(super) fn use_conflicts(game:&Game<'_>)->bool {
    if !game.right {return false;}
    fn name(s:Option<&horizon_client::items::ItemStack>)->Option<&str>{s.map(|s|util::short(&s.item))}
    let main=game.inventory.get(game.selected).and_then(|s|s.as_ref());
    if matches!(name(main),Some("respawn_anchor"|"glowstone")) {return true;}
    let off=game.offhand.as_ref();
    !matches!(name(off),Some("end_crystal"|"obsidian")) && !combat_item(game,off)
}
pub(super) fn combat_item(game:&Game<'_>,stack:Option<&horizon_client::items::ItemStack>)->bool {
    stack.is_none_or(|s|s.count==0||matches!(util::short(&s.item),"mace"|"totem_of_undying")
        ||["minecraft:swords","minecraft:axes"].iter().any(|tag|game.items().item_tags.get(*tag).is_some_and(|items|items.contains(&*s.item))))
}
pub(super) fn friend_damage(game:&Game<'_>,target:&Target,blast:super::sunset_explosive_damage::Blast,budget:&mut super::sunset_explosive_policy::Budget)->Option<f64> {
    use super::sunset_explosive_damage as damage;
    let sample=damage::target_hurt_raw_at(game,target,blast,budget)?;
    damage::reduce_target_hurt_raw(game,target,sample,sample.raw_before_armor())
}
