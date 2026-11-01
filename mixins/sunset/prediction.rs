// Original native forecast and offensive hurt ledger. Authoritative entities
// stay untouched; all geometry is copied and collision work is bounded.
use super::{Game,Dim};
use super::sunset_util::{self as util,Target};
use super::sunset_lead_policy as lead;
use super::sunset_hurt_policy as hurt;
use super::sunset_explosive_damage::{self as damage,Blast};
use super::sunset_explosive_policy::Budget;
use super::sunset_prediction_native::{StrictChunks,NativeKernel};
use horizon_client::collisions::{SceneCollisions,EntityCollisions,PowderSnowSubject};
use horizon_core::geometry::{Aabb,Vec3};
use horizon_world::{collision_context::{CollisionContext,CollisionEntity,CollisionItem},entity_obstruction::EntityIdentity};
use horizon_protocol::{entity_data::Value,packets::play::{Clientbound as C,Serverbound as S}};
use std::sync::{Arc,Mutex};
type Session=Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
struct Scene {session:Session,dim:Dim,id:i32}
#[derive(Clone,Copy)]
struct Pending {source:hurt::ShotSource,victim:u32,raw:f32,tick:u64,lead:u32}
struct Hands {id:u32,stacks:[Option<horizon_client::items::ItemStack>;2],using:Option<(usize,u64)>}
struct State {
    scene:Option<Scene>,generation:u64,tick:u64,steps:u32,tracks:lead::Tracks,
    cache:Vec<(u32,u32,Target)>,absorption:Vec<(u32,f32)>,clock:hurt::Clock,
    receipts:hurt::ReceiptTiming,pending:Vec<Pending>,
    hands:Vec<Hands>,
}
impl Default for State {fn default()->Self {Self {scene:None,generation:0,tick:0,steps:512,
    tracks:lead::Tracks::default(),cache:Vec::new(),absorption:Vec::new(),clock:hurt::Clock::new(),
    receipts:hurt::ReceiptTiming::new(),pending:Vec::new(),hands:Vec::new()}}}
static STATE:Mutex<Option<State>>=Mutex::new(None);
fn state<R>(f:impl FnOnce(&mut State)->R)->R {
    let mut guard=STATE.lock().unwrap_or_else(|e|e.into_inner()); f(guard.get_or_insert_with(State::default))
}
fn same(game:&Game<'_>,scene:&Scene)->bool {
    scene.dim==game.dim && game.remote.as_ref().is_some_and(|r|
        r.entity_id==scene.id && Arc::ptr_eq(&r.session.counters,&scene.session))
}
pub(super) fn reset() {state(|s| {let generation=s.generation.wrapping_add(1);*s=State{generation,..State::default()};});}
pub(super) fn tick(game:&Game<'_>) {
    state(|s| {
        let changed=s.scene.as_ref().is_none_or(|scene|!same(game,scene));
        if changed {
            let generation=s.generation.wrapping_add(1);
            *s=State{generation,scene:game.remote.as_ref().map(|r|Scene{
                session:r.session.counters.clone(),dim:game.dim,id:r.entity_id}),..State::default()};
        }
        if changed || s.tick!=game.ticks {
            s.tick=game.ticks;s.steps=512;s.cache.clear();
            s.clock.tick(s.generation,game.ticks,|id|game.mobs.iter().any(|m|m.id==id&&m.alive()&&!m.removed));
        }
        s.pending.retain(|p|p.tick==game.ticks);
    });
}
pub(super) fn reset_receipts() {state(|s|s.receipts.reset());}
pub(super) fn rtt(game:&Game<'_>)->u32 {
    state(|s|s.receipts.rtt_ticks(super::remote::sunset_combat_remote::latency(game).unwrap_or(0)))
}
pub(super) fn effective_health(game:&Game<'_>,target:&Target)->Option<f64> {
    let extra=state(|s|s.absorption.iter().find(|p|p.0==target.id).map(|p|p.1))?;
    let hp=f64::from(target.health+extra);
    (hp.is_finite()&&hp>0.0).then_some(hp)
}
// Some(None) means no use was observed. None means an active hand's complete
// equipment has not arrived; that cannot establish a shield damage reduction.
pub(super) fn blocking_hand(id:u32,tick:u64)->Option<Option<(horizon_client::items::ItemStack,u64)>> {
    state(|s| {
        let hand=s.hands.iter().find(|h|h.id==id)?;
        let Some((slot,since))=hand.using else {return Some(None);};
        Some(Some((hand.stacks[slot].clone()?,tick.checked_sub(since)?)))
    })
}
fn vec(value:[f64;3])->Vec3 {Vec3::new(value[0],value[1],value[2])}
fn context<'a>(game:&'a Game<'_>,target:&Target)->Option<CollisionContext<'a>> {
    let mob=game.mobs.iter().find(|m|m.id==target.id&&!m.removed)?;
    let held=mob.extra.main_hand.as_deref().map(|item|CollisionItem{item,count:1}).unwrap_or(CollisionItem::EMPTY);
    Some(CollisionContext::for_entity(CollisionEntity{identity:EntityIdentity(u64::from(target.id)),
        y:mob.position.y,descending:mob.extra.crouching,main_hand:Some(held)},false))
}
fn subject(game:&Game<'_>,id:u32)->Option<PowderSnowSubject> {
    let mob=game.mobs.iter().find(|m|m.id==id)?;
    Some(PowderSnowSubject{fall_distance:mob.fall_distance,
        leather_boots:mob.extra.feet.as_deref()==Some("minecraft:leather_boots"),..Default::default()})
}
pub(super) fn clip_body(game:&Game<'_>,target:&Target,from:[f64;3],attempt:[f64;3])->Option<[f64;3]> {
    use lead::CollisionKernel;
    if !from.iter().chain(attempt.iter()).all(|v|v.is_finite()) || vec(attempt).length_squared()>256.0 {return None;}
    let bounds=Aabb::new(vec(target.lo),vec(target.hi)).move_by(vec(target.pos).vector_to(vec(from)));
    let mob=game.mobs.iter().find(|m|m.id==target.id)?;
    let blocks=SceneCollisions{world:&game.world,binding:game.collision,subject:subject(game,target.id)};
    let mut world=StrictChunks(EntityCollisions::with_border(blocks,game.border.geometry(),[mob.position.x,mob.position.z]));
    NativeKernel{world:&mut world,identity:EntityIdentity(u64::from(target.id)),
        context:context(game,target)?,cell_budget_per_step:512}.clip(bounds,vec(attempt)).ok().map(|v|v.components())
}
pub(super) fn target(game:&Game<'_>,target:&Target,ahead:u32)->Option<Target> {
    if let Some(cached)=state(|s|s.cache.iter().find(|p|p.0==target.id&&p.1==ahead).map(|p|p.2.clone())) {return Some(cached);}
    let observed=super::remote::sunset_prediction_remote::motion(game,target.id)?;
    let motion=state(|s|s.tracks.observe(target.id,game.ticks,observed.server,observed.update_interval).ok())?;
    let snapshot=lead::Snapshot{rendered_feet:observed.rendered,server_feet:observed.server,
        rendered_box:observed.bounds,motion,floating:observed.floating};
    let flight=super::sunset_crystal_bot::flight_offset(game,target.id,ahead).map(vec);
    let projected=if let Some(offset)=flight {lead::with_offset(snapshot,offset).ok()?} else {
        let blocks=SceneCollisions{world:&game.world,binding:game.collision,subject:subject(game,target.id)};
        let mut world=StrictChunks(EntityCollisions::with_border(blocks,game.border.geometry(),[observed.rendered.x,observed.rendered.z]));
        let mut kernel=NativeKernel{world:&mut world,identity:EntityIdentity(u64::from(target.id)),
            context:context(game,target)?,cell_budget_per_step:512};
        state(|s|lead::project(snapshot,ahead,&mut s.steps,&mut kernel).ok())?
    };
    let mut out=target.clone();out.pos=projected.feet.components();
    out.lo=projected.bounds.min_position().components();out.hi=projected.bounds.max_position().components();
    state(|s| {if s.cache.len()<96 {s.cache.push((out.id,ahead,out.clone()));}});
    Some(out)
}
pub(super) fn local_offset(game:&Game<'_>,ahead:u32)->Option<[f64;3]> {
    let local=super::remote::sunset_prediction_remote::local_motion(game)?;
    let player=game.player.as_ref()?;
    let held=game.inventory.get(game.selected).and_then(Option::as_ref)
        .map(|s|CollisionItem{item:&s.item,count:s.count as i32}).unwrap_or(CollisionItem::EMPTY);
    let blocks=SceneCollisions{world:&game.world,binding:game.collision,subject:Some(player.powder_snow_subject())};
    let mut world=StrictChunks(EntityCollisions::with_border(blocks,game.border.geometry(),[player.position.x,player.position.z]));
    let mut kernel=NativeKernel{world:&mut world,identity:EntityIdentity(0),
        context:CollisionContext::for_entity(CollisionEntity{identity:EntityIdentity(0),y:player.position.y,
            descending:player.shift_key_down,main_hand:Some(held)},false),cell_budget_per_step:512};
    let snapshot=lead::Snapshot{rendered_feet:local.feet,server_feet:local.feet,rendered_box:local.bounds,
        motion:lead::Motion{velocity:local.previous.vector_to(local.feet),extra_ticks:0},floating:local.floating};
    state(|s|lead::project(snapshot,ahead,&mut s.steps,&mut kernel).ok()).map(|p|p.offset.components())
}
pub(super) fn face_facts(game:&Game<'_>,target:&Target)->super::sunset_faceplace_policy::FaceFacts {
    use super::{sunset_faceplace_native as native,sunset_durability_patch::Prototype};
    let armor=game.mobs.iter().find(|m|m.id==target.id).map_or([None;4],|mob|std::array::from_fn(|i|{
        let stack=mob.extra.armor[i].as_ref()?;let info=game.items().get(&stack.item)?;
        let name=stack.item.strip_prefix("minecraft:")?;
        if !["helmet","chestplate","leggings","boots"].iter().any(|suffix|name.ends_with(suffix)) && name!="elytra" {return None;}
        native::armor(stack,Prototype{maximum:(info.max_damage>0).then_some(info.max_damage),damage_present:true,unbreakable:false})
    }));
    super::sunset_faceplace_policy::FaceFacts{effective_health:effective_health(game,target),armor,
        neighbor_resistance:native::horizontal_resistance(&game.world,game.res.facts(),vec(target.pos))}
}
pub(super) fn estimate(game:&Game<'_>,victim:&Target,blast:Blast,ahead:u32,lead:u32,budget:&mut Budget)->Option<(f64,f32)> {
    let sample=damage::target_hurt_raw_at(game,victim,blast,budget)?;
    let assessed=state(|s|s.clock.assess(victim.id,sample.raw_before_armor(),ahead,lead));
    Some((damage::reduce_target_hurt_raw(game,victim,sample,assessed.raw_for_reduction)?,sample.raw_before_armor()))
}
pub(super) fn opens_in(victim:u32,lead:u32)->u32 {state(|s|s.clock.opens_in(victim,lead))}
pub(super) fn arm_blast(game:&Game<'_>,source:hurt::ShotSource,blast:Blast,
    targets:&[Target],selected:u32,horizon:u32,budget:&mut Budget)->bool {
    let mut selected_ready=false;
    let mut samples=Vec::with_capacity(4);
    for victim in targets.iter().take(4) {
        let Some(projected)=target(game,victim,horizon) else {continue;};
        let Some((_,raw))=estimate(game,&projected,blast,0,rtt(game),budget) else {continue;};
        samples.push((victim.id,raw));
        selected_ready|=victim.id==selected;
    }
    if !selected_ready {return false;}
    for (id,raw) in samples {arm(game,source,id,raw,rtt(game));}
    true
}
pub(super) fn arm(game:&Game<'_>,source:hurt::ShotSource,victim:u32,raw:f32,lead:u32) {
    state(|s| {if s.pending.len()<8{s.pending.push(Pending{source,victim,raw,tick:game.ticks,lead});}});
}
pub(super) fn forwarded(game:&Game<'_>,packet:&S) {
    state(|s| {
        let mut i=0;
        while i<s.pending.len() {
            let pending=s.pending[i];
            let matches=match (pending.source,packet) {
                (hurt::ShotSource::Crystal(id),S::Attack(p))=>p.id==id as i32,
                (hurt::ShotSource::Anchor(center),S::UseItemOn(p))=>p.pos.map(|n|f64::from(n)+0.5)==center,
                _=>false,
            };
            if matches&&pending.tick==game.ticks {s.clock.sent(pending.victim,pending.raw,pending.source,pending.lead);s.pending.swap_remove(i);} else {i+=1;}
        }
        if let S::UseItemOn(p)=packet
            && matches!(&*util::facts(game,p.pos).name,"minecraft:obsidian"|"minecraft:bedrock")
            && game.stack_in(if p.hand==1 {util::OFFHAND}else{game.selected}).is_some_and(|stack|&*stack.item=="minecraft:end_crystal") {s.receipts.placed(p.pos,game.ticks);}
    });
}
pub(super) fn packet(game:&Game<'_>,packet:&C) {
    if matches!(packet,C::Login(_)|C::Respawn(_)|C::Disconnect(_)) {reset();return;}
    tick(game);
    // Incoming packets are observed before native application. Drop all cached
    // copied actor facts so a deferred deed samples the updated native state.
    // Keep the shared collision budget: invalidation must not replenish it.
    state(|s|s.cache.clear());
    if let Some(notice)=super::remote::sunset_hurt_remote::damage_notice(game,packet,|id|
        game.mobs.iter().find(|m|m.id==id).is_some_and(|m|util::player_enemy(game,m)))
        && let Some(id)=game.remote.as_ref().and_then(|r|u32::try_from(r.entity_id).ok()) {state(|s|s.clock.damaged(id,notice));}
    if let Some(ids)=super::remote::sunset_hurt_remote::removed_ids(packet) {state(|s| {
        s.clock.removed(&ids);for id in &ids{s.tracks.forget(*id);}s.absorption.retain(|p|!ids.contains(&p.0));s.cache.retain(|p|!ids.contains(&p.0));
        s.hands.retain(|p|!ids.contains(&p.id));
    });}
    if let Some((id,equipment))=super::remote::sunset_prediction_remote::equipment(game,packet) {state(|s| {
        if let Some(hand)=s.hands.iter_mut().find(|h|h.id==id) {
            for (slot,stack) in equipment {if hand.stacks[slot]!=stack {
                hand.stacks[slot]=stack;
                if hand.using.is_some_and(|(active,_)|active==slot) {hand.using=Some((slot,game.ticks));}
            }}
        }
    });}
    if let Some(base)=super::remote::sunset_hurt_remote::spawned_base(game,packet) {state(|s|{s.receipts.spawned(base,game.ticks);});}
    match packet {
        C::AddEntity(p) if p.id>=0 && super::remote::sunset_prediction_remote::is_player(game,p.kind)=>state(|s| {
            let id=p.id as u32;s.tracks.forget(id);s.absorption.retain(|p|p.0!=id);
            if s.absorption.len()<512 {s.absorption.push((id,0.0));}
            s.hands.retain(|h|h.id!=id);
            if s.hands.len()<512{s.hands.push(Hands{id,stacks:[None,None],using:None});}
        }),
        C::SetEntityData(p) if p.id>=0=>state(|s| {
            if let Some(extra)=s.absorption.iter_mut().find(|pair|pair.0==p.id as u32) {
                for (index,_,value) in &p.values {if let (17,Value::Float(amount))=(index,value) {
                    extra.1=if amount.is_finite()&&*amount>=0.0 {*amount}else{f32::NAN};
                }}
            }
            if let Some(hand)=s.hands.iter_mut().find(|h|h.id==p.id as u32) {
                for (index,_,value) in &p.values {if let (8,Value::Byte(flags))=(index,value) {
                    let slot=usize::from(flags&2!=0);
                    if flags&1==0 {hand.using=None;}
                    else if !hand.using.is_some_and(|(active,_)|active==slot) {hand.using=Some((slot,game.ticks));}
                }}
            }
        }),
        _=>{},
    }
}
