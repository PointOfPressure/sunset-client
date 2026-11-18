// Entity Control: after the boat you drive has ticked, its move is made again with the motion Sunset gives it, and
// you sit in it where it now is.
fn sunset_boat_ride(this: &mut Game<'_>, keys: Keys, sprint_window: i32) -> std::ops::ControlFlow<Result<bool>> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    static BUSY: AtomicBool = AtomicBool::new(false);
    if BUSY.load(Relaxed) || this.riding.is_none_or(|id| !this.boats.iter().any(|b| b.id == id)) {
        return std::ops::ControlFlow::Continue(());
    }
    BUSY.store(true, Relaxed);
    let value = this.remote_ride(keys, sprint_window);
    BUSY.store(false, Relaxed);
    if value.is_err() {
        return std::ops::ControlFlow::Break(value);
    }
    let Some(i) = this.riding.and_then(|id| this.boats.iter().position(|b| b.id == id)) else {
        return std::ops::ControlFlow::Break(value);
    };
    let mut b = this.boats[i].clone();
    let cell = [b.position.x.floor() as i32, (b.position.y + 0.1).floor() as i32, b.position.z.floor() as i32];
    let f = crate::game::sunset_util::facts(this, cell);
    let in_water = f.fluid == crate::game::Fluid::Water || f.waterlogged;
    let Some((v, yaw)) = super::sunset_vehicle::boat(this, b.velocity, b.on_ground, in_water) else {
        return std::ops::ControlFlow::Break(value);
    };
    if let Some(yaw) = yaw {
        b.yaw = yaw;
    }
    let obstacles = this.boat_move_obstacles(&b);
    b.position = b.previous;
    let mut scene = horizon_client::collisions::EntityCollisions {
        blocks: horizon_client::collisions::SceneCollisions { world: &this.world, binding: this.collision, subject: None },
        entities: &obstacles,
        border: None,
    };
    if let Ok(moved) = scene.resolve(b.bounds(), v, 0.0, b.on_ground) {
        b.position = Vec3::new(b.position.x + moved.x, b.position.y + moved.y, b.position.z + moved.z);
        b.on_ground = v.y < 0.0 && moved.y > v.y;
        let keep = |want: f64, got: f64| if (got - want).abs() > 1.0e-7 { 0.0 } else { want };
        b.velocity = Vec3::new(keep(v.x, moved.x), keep(v.y, moved.y), keep(v.z, moved.z));
    }
    this.boats[i] = b;
    let seat = this.player_boat_seat(i);
    if let Some(p) = this.player.as_mut() {
        p.position = seat;
    }
    std::ops::ControlFlow::Break(value)
}
