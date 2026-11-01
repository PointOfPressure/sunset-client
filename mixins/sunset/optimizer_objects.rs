// Most picks return the exact original native result. Only a masked nearest
// crystal needs a second native nonattack pick and the attack-only backfill.
fn sunset_optimizer_objects(game:&crate::game::Game<'_>,attacking:bool)
    ->std::ops::ControlFlow<Option<ObjectHit>> {
    thread_local!{static SUNSET_OBJECT_PASS:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
    if !attacking || SUNSET_OBJECT_PASS.with(|flag|flag.get())
        || !crate::game::sunset_crystal_optimizer::has_predictions(game){
        return std::ops::ControlFlow::Continue(());
    }
    struct Reset;
    impl Drop for Reset{fn drop(&mut self){SUNSET_OBJECT_PASS.with(|flag|flag.set(false));}}
    SUNSET_OBJECT_PASS.with(|flag|flag.set(true));let _reset=Reset;
    let first=game.remote_object_hit(true);
    let masked=first.as_ref().is_some_and(|hit|u32::try_from(hit.id).ok()
        .and_then(|id|game.crystals.iter().find(|crystal|crystal.id==id))
        .is_some_and(|crystal|crate::game::sunset_crystal_optimizer::hidden_id(
            game,crystal.id,[crystal.position.x,crystal.position.y,crystal.position.z])));
    if !masked{return std::ops::ControlFlow::Break(first);}
    // All paintings, frames, stands, cushions, boats and carts still use their
    // original native enumeration, bounds and nearest-hit tuple unchanged.
    let mut best=game.remote_object_hit(false);
    let Some((eye,direction))=game.aim() else{return std::ops::ControlFlow::Break(best);};
    let reach=if game.creative{5.0}else{3.0};
    let mut consider=|id:i32,distance:f64,origin:[f64;3]| {
        if best.as_ref().is_none_or(|old|distance.total_cmp(&old.distance).is_lt()) {
            best=Some(ObjectHit{id,distance,location:std::array::from_fn(|i|eye[i]+direction[i]*distance-origin[i])});
        }
    };
    for crystal in game.crystals.iter().filter(|crystal|crystal.id!=0) {
        let pos=crystal.position;let origin=[pos.x,pos.y,pos.z];
        if crate::game::sunset_crystal_optimizer::hidden_id(game,crystal.id,origin){continue;}
        let bounds=Aabb::new(Vec3::new(pos.x-1.0,pos.y,pos.z-1.0),Vec3::new(pos.x+1.0,pos.y+2.0,pos.z+1.0));
        if let Some(distance)=ray_box(eye,direction,reach,bounds){consider(crystal.id as i32,distance,origin);}
    }
    // Native dragon-part rays remain authoritative; their order follows the
    // crystal order exactly as in the original attacking-only enumeration.
    if let Some((id,distance,origin))=game.remote_dragon_part_hit(eye,direction,reach){consider(id,distance,origin);}
    std::ops::ControlFlow::Break(best)
}
