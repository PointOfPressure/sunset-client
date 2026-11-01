// Register inside game/remote.rs. Thin access to native validated preview and
// mining state; no packet-only shortcut or guessed break speed.
use crate::game::{Game, Hit};

struct ScopedHit<'a, 'r> {
    game: &'a mut Game<'r>,
    previous: Option<Hit>,
}
impl Drop for ScopedHit<'_, '_> {
    fn drop(&mut self) {
        self.game.forced_hit = self.previous;
    }
}

pub(crate) fn step(game:&mut Game<'_>,position:[i32;3],face:[i32;3],yaw:f32,pitch:f32)->bool {
    let range=game.reach();step_with_range(game,position,face,yaw,pitch,range)
}
pub(crate) fn step_with_range(
    game: &mut Game<'_>,
    position: [i32; 3],
    face: [i32; 3],
    yaw: f32,
    pitch: f32,
    range:f64,
) -> bool {
    if !range.is_finite()||range<=0.0||range>64.0{return false;}
    let Some(eye) = game.eye_position() else {
        return false;
    };
    if !eye.iter().all(|v| v.is_finite()) || !yaw.is_finite() || !pitch.is_finite() {
        return false;
    }
    let direction = crate::game::view_vector(yaw, pitch);
    let Some(hit) = game.clip_ray(eye, direction, range) else {
        return false;
    };
    if hit.pos != position || hit.face != face {
        return false;
    }
    let start = game
        .remote
        .as_ref()
        .is_none_or(|r| r.destroying.is_none_or(|d| d.0 != position));
    let previous = game.forced_hit;
    game.forced_hit = Some(hit);
    let guard = ScopedHit { game, previous };
    guard.game.remote_mining(start);
    true
}
pub(crate) fn stop(game: &mut Game<'_>, position: [i32; 3]) {
    if game
        .remote
        .as_ref()
        .is_some_and(|r| r.destroying.is_some_and(|d| d.0 == position))
    {
        game.stop_remote_mining();
    }
    if game.mining.as_ref().is_some_and(|m| m.pos == position) {
        game.mining = None;
    }
}
pub(crate) fn face_index(face: [i32; 3]) -> i32 {
    i32::from(super::face_direction(face))
}
pub(crate) fn progress(game: &Game<'_>, position: [i32; 3]) -> Option<f32> {
    game.remote
        .as_ref()?
        .destroying
        .filter(|d| d.0 == position)
        .map(|d| d.2)
}
pub(crate) fn busy(game: &Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|r| r.destroying.is_some())
}
pub(crate) fn predicted(game: &Game<'_>, position: [i32; 3]) -> bool {
    game.remote
        .as_ref()
        .is_some_and(|r| r.predictions.contains_key(&position))
}
pub(crate) fn preview(
    game: &mut Game<'_>,
    desired: [i32; 3],
    clicked: [i32; 3],
    face: [i32; 3],
    point: [f64; 3],
    slot: usize,
) -> bool {
    let Some(stack) = game.inventory.get(slot).and_then(|s| s.as_ref()).cloned() else {
        return false;
    };
    if slot >= 9 || stack.count == 0 {
        return false;
    }
    let hit = Hit {
        pos: clicked,
        face,
        point,
    };
    game.remote_can_place(&hit, &stack)
        .is_some_and(|(pos, _)| [pos.x, pos.y, pos.z] == desired)
}

// Native validated block use with its own prediction, sequence, components,
// creative count behavior and local swing. Never write a guessed world state.
pub(crate) fn place(game:&mut Game<'_>,desired:[i32;3],clicked:[i32;3],face:[i32;3],point:[f64;3])->bool {
    if !preview(game,desired,clicked,face,point,game.selected){return false;}
    let before=game.world.block(desired[0],desired[1],desired[2]);
    let previous=game.forced_hit;game.forced_hit=Some(Hit{pos:clicked,face,point});
    let guard=ScopedHit{game,previous};
    let done=guard.game.remote_use_block(horizon_client::first_person::Hand::Main);
    done && guard.game.world.block(desired[0],desired[1],desired[2])!=before
        && guard.game.remote.as_ref().is_some_and(|remote|remote.predictions.contains_key(&desired))
}
