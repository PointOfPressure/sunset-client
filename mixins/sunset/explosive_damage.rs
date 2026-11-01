// Original explosive prediction using Horizon collision shapes and damage kernels.
// Pure observations: never wears equipment, applies damage or changes the world.
use super::Game;
use super::sunset_explosive_policy::{Budget, armor_survives, in_range, raw_damage, samples};
use super::sunset_util::Target;
use horizon_client::collisions::SceneCollisions;
use horizon_core::{geometry::{Vec3, VoxelShape}, position::BlockPos};
use horizon_world::{
    collision_context::{CollisionContext, CollisionEntity, CollisionItem},
    combat_rules::ArmorWorld,
    damage_reduction::{self, Bypass, DamageReductionWorld},
    damage_source::{DamageSource, Scaling},
    entity_obstruction::EntityIdentity,
    food::Difficulty,
    registry::BlockStateId,
    world_ray::{RayBlockMode, RayFluidMode, RayWorld, clip_world},
};
use std::io;

#[derive(Clone, Copy, Debug)]
pub(super) struct Blast {
    pub center: [f64; 3],
    pub power: f32,
    // Ignore exactly one destroyed block without changing the damage source.
    pub ignored_block: Option<[i32; 3]>,
    virtual_block: Option<([i32;3],BlockStateId)>,
    kind: BlastKind,
}

#[derive(Clone, Copy, Debug)]
enum BlastKind { Explosion, BadRespawnPoint }
impl Blast {
    pub(super) fn explosion(center: [f64; 3], power: f32) -> Self {
        Self { center, power, ignored_block: None, virtual_block:None, kind: BlastKind::Explosion }
    }
    pub(super) fn anchor(block: [i32; 3]) -> Self {
        Self { center: block.map(|v| f64::from(v)+0.5), power: 5.0,
            ignored_block: Some(block), virtual_block:None, kind: BlastKind::BadRespawnPoint }
    }
    pub(super) fn ignoring(mut self, block: [i32; 3]) -> Self {
        self.ignored_block = Some(block);
        self
    }
    pub(super) fn with_block(mut self,pos:[i32;3],state:BlockStateId)->Self {
        self.virtual_block=Some((pos,state));self
    }
    fn source(self) -> &'static str {
        match self.kind { BlastKind::Explosion => "minecraft:explosion", BlastKind::BadRespawnPoint => "minecraft:bad_respawn_point" }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Prediction {
    // Current observed defenses; remote hidden effects/attributes are unknown.
    pub estimate: f64,
    // No armor/effect credit: conservative self/friend check, including armor
    // breakage during the hit. Still conditional on observed world and position.
    pub upper: f64,
    // Local observed defenses applied to the same Hard raw bound, only when
    // every worn piece is known to survive the hit. Remote victims get no credit.
    pub equipped_upper: f64,
}

pub(super) fn safety_damage(prediction: Prediction, mode: &str) -> f64 {
    if mode == "Observed equipment" { prediction.equipped_upper } else { prediction.upper }
}

// Engine collider shape dispatch, including contextual blocks, rather than
// treating every occupied block as a full cube. Missing chunks/errors abort the
// entire prediction; they must never become false evidence of protective cover.
struct Rays<'a, 'r> {
    scene: SceneCollisions<'a, 'r>,
    context: CollisionContext<'static>,
    ignored: Option<[i32; 3]>,
    virtual_block:Option<([i32;3],BlockStateId)>,
}
impl RayWorld for Rays<'_, '_> {
    type Error = io::Error;
    type Block = BlockStateId;
    type Fluid = ();
    fn block_state(&mut self, p: BlockPos) -> io::Result<BlockStateId> {
        if !self.scene.world.loaded(p.x, p.z) { return Err(io::Error::other("unloaded explosive ray")); }
        if let Some((pos,state))=self.virtual_block && pos==[p.x,p.y,p.z] {return Ok(state);}
        Ok(self.scene.world.block(p.x, p.y, p.z))
    }
    fn fluid_state(&mut self, _: BlockPos) -> io::Result<()> { Ok(()) }
    fn block_shape(&mut self, state: BlockStateId, p: BlockPos, _: RayBlockMode) -> io::Result<VoxelShape> {
        if self.ignored == Some([p.x, p.y, p.z]) { return Ok(VoxelShape::empty()); }
        let binding = self.scene.binding;
        binding.collision_shape_with_context(&mut self.scene, state, p, &self.context)
            .map_err(|e| io::Error::other(format!("{e:?}")))
    }
    fn interaction_shape(&mut self, _: BlockStateId, _: BlockPos) -> io::Result<VoxelShape> { Ok(VoxelShape::empty()) }
    fn fluid_is_source(&mut self, _: ()) -> io::Result<bool> { Ok(false) }
    fn fluid_is_empty(&mut self, _: ()) -> io::Result<bool> { Ok(true) }
    fn fluid_is_water(&mut self, _: ()) -> io::Result<bool> { Ok(false) }
    fn fluid_shape(&mut self, _: (), _: BlockPos) -> io::Result<VoxelShape> { Ok(VoxelShape::empty()) }
}

fn rays<'a, 'r>(game: &'a Game<'r>, y: f64, ignored: Option<[i32; 3]>) -> Rays<'a, 'r> {
    Rays {
        scene: SceneCollisions { world: &game.world, binding: game.collision, subject: None },
        context: CollisionContext::for_entity(CollisionEntity {
            identity: EntityIdentity(0), y, descending: false, main_hand: Some(CollisionItem::EMPTY),
        }, false),
        ignored,
        virtual_block:None,
    }
}

fn clear(rays: &mut Rays<'_, '_>, from: [f64; 3], to: [f64; 3]) -> Option<bool> {
    if !in_range(from, to, 64.0)
        || !from.iter().chain(to.iter()).all(|v| v.abs() <= 30_000_000.0) { return None; }
    clip_world(rays, Vec3::new(from[0], from[1], from[2]), Vec3::new(to[0], to[1], to[2]),
        RayBlockMode::Collider, RayFluidMode::None, 4096).ok().map(|hit| hit.miss)
}

pub(super) fn clear_line(game: &Game<'_>, from: [f64; 3], to: [f64; 3],
    ignored: Option<[i32; 3]>, budget: &mut Budget) -> bool {
    if !budget.line() { return false; }
    clear(&mut rays(game, from[1], ignored), from, to) == Some(true)
}

fn raw(game: &Game<'_>, feet: [f64; 3], min: [f64; 3], max: [f64; 3],
    blast: Blast, budget: &mut Budget) -> Option<f32> {
    if !blast.center.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0)
        || !feet.iter().all(|v| v.is_finite()) { return None; }
    let distance = (0..3).map(|i| (feet[i] - blast.center[i]).powi(2)).sum::<f64>().sqrt();
    let unoccluded = raw_damage(distance, blast.power, 1.0)?;
    if unoccluded == 0.0 {
        return budget.prediction(0).then_some(0.0);
    }
    let points = samples(min, max)?;
    if !budget.prediction(points.len()) { return None; }
    let mut ray = rays(game, min[1], blast.ignored_block);
    ray.virtual_block=blast.virtual_block;
    let mut seen = 0;
    for point in &points {
        if clear(&mut ray, *point, blast.center)? { seen += 1; }
    }
    raw_damage(distance, blast.power, seen as f32 / points.len() as f32)
}

// The source catalog decides difficulty and bypass tags; do not assume that
// all explosion-like sources scale the same way on a customized server.
fn source<'a>(game: &'a Game<'_>, blast: Blast) -> Option<DamageSource<'a, ()>> {
    let catalog = &game.motion.health.damage;
    catalog.source(catalog.id(blast.source())?, None, None, None)
}

fn difficulty(damage: f32, difficulty: Difficulty, scaling: Scaling) -> f32 {
    // Crystals/anchors have no living non-player causing entity.
    if scaling != Scaling::Always { return damage; }
    match difficulty {
        Difficulty::Peaceful => 0.0,
        Difficulty::Easy => (damage / 2.0 + 1.0).min(damage),
        Difficulty::Normal => damage,
        Difficulty::Hard => damage * 3.0 / 2.0,
    }
}

fn difficulty_id(id: u8) -> Difficulty {
    match id {
        0 => Difficulty::Peaceful,
        1 => Difficulty::Easy,
        3 => Difficulty::Hard,
        _ => Difficulty::Normal,
    }
}

fn observed_difficulty(game: &Game<'_>) -> Difficulty {
    // Remote ChangeDifficulty updates the UI snapshot, not World.health_rules.
    if game.remote.is_some() { difficulty_id(game.ui.difficulty) }
    else { game.world.health_rules.difficulty }
}

fn upper_damage(raw: f32, observed: Difficulty, scaling: Scaling, remote: bool) -> f32 {
    // There is no separate "difficulty packet received" flag. A remote
    // snapshot can therefore still hold its default or a previous connection's
    // value. Hard is the conservative player bound, even on Normal/Easy.
    // Sources marked Never/LivingNonPlayer still follow their catalog scaling.
    difficulty(raw, if remote { Difficulty::Hard } else { observed }, scaling)
}

#[derive(Clone, Copy, Debug)]
struct EquipmentProof { known: bool, survives: bool, reason: &'static str, slot: Option<usize> }
fn equipment_proof(game: &Game<'_>, damage: f32) -> EquipmentProof {
    for (slot, stack) in game.armor.iter().enumerate().filter_map(|(slot, stack)| Some((slot, stack.as_ref()?))) {
        let fail = |known, reason| EquipmentProof { known, survives: false, reason, slot: Some(slot) };
        let Some(info) = game.items().get(&stack.item) else { return fail(false,"unknown-item"); };
        if stack.count != 1 { return fail(false,"stack-count"); }
        if stack.extra.as_ref().is_some_and(|extra| !extra.other.is_empty()) { return fail(false,"unmodelled-components"); }
        if stack.wire_patch().is_some_and(|patch| !known_equipment_patch(&patch)) { return fail(false,"wire-patch"); }
        // The engine's item_damage effects are data driven. Only direct
        // remove_binomial effects are proved not to increase the wear amount;
        // unknown enchantments or effect compositions receive no armor credit.
        let nonincreasing = stack.extra.as_ref().is_none_or(|extra| extra.enchantments.iter().all(|(id, _)| {
            let Some(enchantment) = game.stations().enchantments.get(id) else { return false; };
            let Some(effects) = enchantment.effects.get("minecraft:item_damage") else { return true; };
            effects.as_array().is_some_and(|list| list.iter().all(|effect|
                effect["effect"]["type"].as_str() == Some("minecraft:remove_binomial")))
        }));
        if !nonincreasing { return fail(false,"item-damage-enchantment"); }
        if !armor_survives(damage, stack.damage, info.max_damage, info.equip_damage_on_hurt, true, true) {
            return fail(true,"durability-or-invalid-damage");
        }
    }
    EquipmentProof { known: true, survives: true, reason: "known-surviving-equipment", slot: None }
}
fn equipment_survives(game: &Game<'_>, damage: f32) -> bool {
    equipment_proof(game, damage).survives
}

fn known_equipment_patch(patch: &[u8]) -> bool {
    use horizon_protocol::{buf::Reader, item::component as c};
    let mut reader = Reader::new(patch);
    let Ok(added) = reader.count(64) else { return false; };
    // Removed component ids can change prototype durability/equipment rules.
    if reader.count(64).ok() != Some(0) { return false; }
    for _ in 0..added {
        let Ok(kind) = reader.varint() else { return false; };
        if !matches!(kind, c::DAMAGE | c::ENCHANTMENTS | c::CUSTOM_NAME | c::REPAIR_COST
            | c::TRIM | c::DYED_COLOR) { return false; }
        let Ok(length) = reader.count(reader.remaining()) else { return false; };
        if reader.bytes(length).is_err() { return false; }
    }
    reader.remaining() == 0
}

// Explicit-command diagnostics only. Report component kind ids and lengths,
// never payload bytes, item NBT, account names or connection details.
fn patch_summary(patch: &[u8]) -> Option<(Vec<(i32, usize)>, Vec<i32>)> {
    use horizon_protocol::buf::Reader;
    let mut r = Reader::new(patch);
    let added = r.count(64).ok()?;
    let removed = r.count(64).ok()?;
    let mut kinds = Vec::with_capacity(added);
    for _ in 0..added {
        let kind = r.varint().ok()?;
        let length = r.count(r.remaining()).ok()?;
        r.bytes(length).ok()?;
        kinds.push((kind,length));
    }
    let removed = (0..removed).map(|_|r.varint()).collect::<Result<Vec<_>,_>>().ok()?;
    (r.remaining()==0).then_some((kinds,removed))
}

pub(super) fn diagnostics(game: &mut Game<'_>, blast: Blast) -> String {
    use std::fmt::Write;
    let Some(p) = game.player.as_mut() else { return "Damage probe: no local player".into(); };
    let armor = p.attribute("minecraft:armor").ok();
    let toughness = p.attribute("minecraft:armor_toughness").ok();
    let resistance = p.status.resistance;
    let protection = p.status.protection(blast.source());
    let feet = [p.position.x,p.position.y,p.position.z];
    let bounds = p.bounds(); let (lo,hi) = (bounds.min_position(),bounds.max_position());
    let mut attributes = String::new();
    for name in ["minecraft:armor","minecraft:armor_toughness"] {
        let key = p.attributes.key(name);
        let handle = p.attributes.lookup(key.as_ref());
        if let Some(instance) = handle.and_then(|handle|p.attributes.instance(&handle).ok()) {
            let modifiers: Vec<_> = instance.modifiers().iter().map(|m|(m.id(),m.amount(),m.operation())).collect();
            let _ = write!(attributes," {name}:base={} modifiers={modifiers:?};",instance.base_value());
        } else { let _ = write!(attributes," {name}:no-instance;"); }
    }
    let mut budget = Budget::new(4);
    let raw = raw(game,feet,[lo.x,lo.y,lo.z],[hi.x,hi.y,hi.z],blast,&mut budget);
    let scaled_upper = raw.and_then(|raw| {
        let scaling = source(game,blast)?.definition().scaling;
        Some(upper_damage(raw,observed_difficulty(game),scaling,game.remote.is_some()))
    });
    let proof = equipment_proof(game,scaled_upper.unwrap_or(f32::NAN));
    let prediction = local_damage(game,blast,&mut budget);
    let mut out = format!("Damage source={} center={:?} power={} raw={raw:?} estimate={:?} upper={:?} equipped_upper={:?} armor={armor:?} toughness={toughness:?} resistance={resistance:?} protection={protection} known={} survives={} fallback={} slot={:?}{}",
        blast.source(),blast.center,blast.power,prediction.map(|p|p.estimate),prediction.map(|p|p.upper),
        prediction.map(|p|p.equipped_upper),proof.known,proof.survives,proof.reason,proof.slot,attributes);
    for (slot, stack) in game.armor.iter().enumerate() {
        let Some(stack) = stack else { let _ = write!(out," gear[{slot}]=empty;"); continue; };
        let info = game.items().get(&stack.item);
        let patch = stack.wire_patch();
        let wire = patch.as_deref().map(patch_summary);
        let other: Vec<_> = stack.extra.as_ref().map(|e|e.other.iter().map(|(name,_)|name.as_str()).collect()).unwrap_or_default();
        let prototypes: Vec<_> = info.map(|i|i.modifiers.iter().filter(|(name,_,_,_,_)|
            matches!(name.as_str(),"minecraft:armor"|"minecraft:armor_toughness")).collect()).unwrap_or_default();
        let _ = write!(out," gear[{slot}]={} count={} damage={}/{} wears={} other={other:?} wire_added(kind,len)_removed={wire:?} prototype={prototypes:?};",
            stack.item,stack.count,stack.damage,info.map_or(0,|i|i.max_damage),info.is_some_and(|i|i.equip_damage_on_hurt));
    }
    out
}

struct Defenses<'a> {
    source: DamageSource<'a, ()>,
    armor: f64,
    toughness: f64,
    resistance: Option<i32>,
    protection: f32,
}
impl ArmorWorld for Defenses<'_> {
    type Error = std::convert::Infallible;
    type Weapon = ();
    type ServerLevel = ();
    fn weapon(&mut self) -> Result<Option<()>, Self::Error> { Ok(None) }
    fn server_level(&mut self) -> Result<Option<()>, Self::Error> { Ok(Some(())) }
    fn modify_armor_effectiveness(&mut self, _: (), _: (), value: f32) -> Result<f32, Self::Error> { Ok(value) }
}
impl DamageReductionWorld for Defenses<'_> {
    fn bypasses(&mut self, kind: Bypass) -> Result<bool, Self::Error> { Ok(self.source.bypasses(kind)) }
    fn hurt_armor(&mut self, _: f32) -> Result<(), Self::Error> { Ok(()) }
    fn armor_value(&mut self) -> Result<i32, Self::Error> { Ok(damage_reduction::armor_from_attribute(self.armor)) }
    fn toughness_attribute(&mut self) -> Result<f64, Self::Error> { Ok(self.toughness) }
    fn has_resistance(&mut self) -> Result<bool, Self::Error> { Ok(self.resistance.is_some()) }
    fn resistance_amplifier(&mut self) -> Result<i32, Self::Error> { Ok(self.resistance.unwrap_or(0)) }
    fn victim_is_server_player(&self) -> bool { false }
    fn award_resisted(&mut self, _: i32) -> Result<(), Self::Error> { Ok(()) }
    fn attacker_is_server_player(&mut self) -> Result<bool, Self::Error> { Ok(false) }
    fn award_dealt_resisted(&mut self, _: i32) -> Result<(), Self::Error> { Ok(()) }
    fn damage_protection(&mut self, _: ()) -> Result<f32, Self::Error> { Ok(self.protection) }
}

fn reduced(mut defenses: Defenses<'_>, damage: f32) -> Option<f64> {
    if ![defenses.armor, defenses.toughness, f64::from(defenses.protection)]
        .iter().all(|x| x.is_finite() && *x >= 0.0)
        || defenses.resistance.is_some_and(|n| !(0..=255).contains(&n)) { return None; }
    let armored = damage_reduction::after_armor(&mut defenses, damage).ok()?;
    let result = damage_reduction::after_magic(&mut defenses, armored).ok()?;
    (result.is_finite() && result >= 0.0).then_some(f64::from(result))
}

pub(super) fn local_damage(game: &mut Game<'_>, blast: Blast, budget: &mut Budget) -> Option<Prediction> {
    local_damage_at(game,blast,[0.0;3],budget)
}
pub(super) fn local_damage_at(game: &mut Game<'_>, blast: Blast, offset: [f64;3], budget: &mut Budget) -> Option<Prediction> {
    if !offset.iter().all(|v|v.is_finite()) {return None;}
    let p = game.player.as_mut()?;
    let armor = p.attribute("minecraft:armor").ok()?;
    let toughness = p.attribute("minecraft:armor_toughness").ok()?;
    let resistance = p.status.resistance;
    let protection = p.status.protection(blast.source());
    let feet = [p.position.x+offset[0], p.position.y+offset[1], p.position.z+offset[2]];
    let bounds = p.bounds().move_by(Vec3::new(offset[0],offset[1],offset[2]));
    let (min, max) = (bounds.min_position(), bounds.max_position());
    let damage = raw(game, feet, [min.x, min.y, min.z], [max.x, max.y, max.z], blast, budget)?;
    let source = source(game, blast)?;
    let scaling = source.definition().scaling;
    let observed = observed_difficulty(game);
    let upper = upper_damage(damage, observed, scaling, game.remote.is_some());
    let estimate = reduced(Defenses { source, armor, toughness, resistance, protection },
        difficulty(damage, observed, scaling))?;
    let equipped_upper = if equipment_survives(game, upper) {
        // Recreate the read-only source adapter; no equipment wear or RNG draws.
        reduced(Defenses { source: self::source(game, blast)?, armor, toughness, resistance, protection }, upper)
            .unwrap_or(f64::from(upper)).min(f64::from(upper))
    } else { f64::from(upper) };
    Some(Prediction { estimate, upper: f64::from(upper), equipped_upper })
}

pub(super) fn target_damage(game: &Game<'_>, target: &Target, blast: Blast, budget: &mut Budget) -> Option<Prediction> {
    let mob = game.mobs.iter().find(|m| m.id == target.id && !m.removed && m.alive())?;
    let damage = raw(game, target.pos, target.lo, target.hi, blast, budget)?;
    let source = source(game, blast)?;
    let scaling = source.definition().scaling;
    let observed = observed_difficulty(game);
    let upper = if target.player {
        upper_damage(damage, observed, scaling, game.remote.is_some())
    } else { damage };
    let damage = if target.player { difficulty(damage, observed, scaling) } else { damage };
    let (worn, tough) = mob.worn_armor();
    let resistance = mob.extra.effects.iter().find(|e| e.0 == "resistance" && e.1 != 0).map(|e| e.2);
    let protection = horizon_client::mobs::gear::protection(mob, horizon_client::mobs::DamageKind::Explosion);
    let estimate = reduced(Defenses { source, armor: f64::from(mob.kind.armor + worn),
        toughness: f64::from(mob.kind.armor_toughness + tough), resistance, protection }, damage)?;
    // Remote attributes/hidden effect amplifiers are not authoritative. Do not
    // call this exact or use estimate as a friend damage/survival upper bound.
    Some(Prediction { estimate, upper: f64::from(upper), equipped_upper: f64::from(upper) })
}

// Offensive target pipeline: difficulty -> native shield -> hurt window ->
// native armor/magic. Self and friend bounds continue to use the full blast.
#[derive(Clone,Copy)]
pub(super) struct TargetHurtRaw {victim:u32,blast:Blast,raw:f32}
impl TargetHurtRaw {pub(super) fn raw_before_armor(self)->f32 {self.raw}}

fn target_blocked(game:&Game<'_>,target:&Target,blast:Blast,scaled:f32)->Option<f32> {
    use horizon_world::blocking_rules::{DamageReduction,TypeMatchWorld};
    let Some((stack,elapsed))=super::sunset_prediction::blocking_hand(target.id,game.ticks)? else {return Some(0.0);};
    let Some(component)=game.blocks_attacks(&stack) else {return Some(0.0);};
    // Native protocol cannot decode a customized blocks_attacks patch. Never
    // manufacture custom component facts from an ordinary shield prototype.
    if stack.extra.as_ref().is_some_and(|e|!e.other.is_empty()){return None;}
    if elapsed<(component.delay_seconds*20.0).round().max(0.0) as u64 {return Some(0.0);}
    let damage_type=blast.source();
    let tags=&game.stations().enchantments.damage_tags;
    let matches=|spec:&str|match spec.strip_prefix('#') {
        Some(tag)=>tags.get(tag).is_some_and(|set|set.contains(damage_type)),None=>spec==damage_type,
    };
    if component.bypassed_by.as_deref().is_some_and(matches){return Some(0.0);}
    let mob=game.mobs.iter().find(|m|m.id==target.id)?;
    let yaw=f64::from(mob.head_yaw).to_radians();
    let delta=[blast.center[0]-target.pos[0],blast.center[2]-target.pos[2]];
    let length=delta[0].hypot(delta[1]);
    let cosine=if length<1e-4 {0.0}else{(-yaw.sin()*delta[0]+yaw.cos()*delta[1])/length};
    let angle=cosine.clamp(-1.0,1.0).acos();
    struct Known;
    impl TypeMatchWorld for Known {type Error=();type Set=bool;
        fn source_in_set(&mut self,value:bool)->Result<bool,()> {Ok(value)}}
    let mut blocked=0.0;
    for reduction in &component.reductions {
        let native=DamageReduction{horizontal_angle:reduction.horizontal_angle,
            types:reduction.types.as_ref().map(|set|set.iter().any(|spec|matches(spec))),
            base:reduction.base,factor:reduction.factor};
        blocked+=native.resolve(&mut Known,scaled,angle).ok()?;
    }
    (blocked.is_finite()&&blocked>=0.0).then_some(blocked.min(scaled))
}
pub(super) fn target_hurt_raw_at(game:&Game<'_>,target:&Target,blast:Blast,budget:&mut Budget)->Option<TargetHurtRaw> {
    game.mobs.iter().find(|m|m.id==target.id&&!m.removed&&m.alive())?;
    let raw=raw(game,target.pos,target.lo,target.hi,blast,budget)?;
    let source=source(game,blast)?;
    let scaled=if target.player {difficulty(raw,observed_difficulty(game),source.definition().scaling)} else {raw};
    let blocked=if target.player {target_blocked(game,target,blast,scaled)?} else {0.0};
    let raw=scaled-blocked;
    (raw.is_finite()&&raw>=0.0).then_some(TargetHurtRaw{victim:target.id,blast,raw})
}
pub(super) fn reduce_target_hurt_raw(game:&Game<'_>,target:&Target,sample:TargetHurtRaw,raw:f32)->Option<f64> {
    if sample.victim!=target.id || !raw.is_finite() || raw<0.0 || raw>sample.raw {return None;}
    let mob=game.mobs.iter().find(|m|m.id==target.id&&!m.removed&&m.alive())?;
    let (armor,toughness)=mob.worn_armor();
    let resistance=mob.extra.effects.iter().find(|e|e.0=="resistance"&&e.1!=0).map(|e|e.2);
    let protection=horizon_client::mobs::gear::protection(mob,horizon_client::mobs::DamageKind::Explosion);
    reduced(Defenses{source:source(game,sample.blast)?,armor:f64::from(mob.kind.armor+armor),
        toughness:f64::from(mob.kind.armor_toughness+toughness),resistance,protection},raw)
}

// A caller that distinguishes open from covered must also distinguish unknown.
// Reuse the existing strict loaded-chunk collider query and shared ray budget.
pub(super) fn known_line(game:&Game<'_>,from:[f64;3],to:[f64;3],
    ignored:Option<[i32;3]>,budget:&mut Budget)->Option<bool> {
    if !budget.line(){return None;}
    clear(&mut rays(game,from[1],ignored),from,to)
}
