//! Original bounded forecasting policy. No entity state is modified.
use horizon_core::geometry::{Aabb, Vec3};

pub const TRACK_CAPACITY: usize = 32;
pub const MAX_FORECAST_STEPS: u32 = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Invalid, Capacity, Budget, Collision }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use { CrystalBreak, CrystalPlace, Anchor }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Horizons { pub others: u32, pub local: u32 }
pub fn horizons(action: Use, rtt: u32) -> Horizons {
    Horizons { others: match action { Use::CrystalPlace => rtt.saturating_mul(2).min(5), _ => rtt.min(5) },
        local: if action == Use::CrystalPlace { rtt.min(5) } else { 0 } }
}
#[derive(Default, Debug)]
pub struct Rtt { measured: Option<f64> }
impl Rtt {
    /// Matched observed crystal spawn minus this client's actual placement tick.
    pub fn placement_observed(&mut self, ticks: u64) {
        if (1..=20).contains(&ticks) {
            self.measured = Some(self.measured.map_or(ticks as f64,
                |old| old * 0.75 + ticks as f64 * 0.25));
        }
    }
    pub fn ticks(&self, own_tab_latency_ms: Option<i32>) -> u32 {
        self.measured.map_or_else(|| own_tab_latency_ms.unwrap_or(0).max(0) as u32 / 50 + 1,
            |v| v.round() as u32)
    }
    pub fn confirm_window(&self, latency: Option<i32>) -> u32 { self.ticks(latency).saturating_add(2).clamp(4, 20) }
    pub fn reset(&mut self) { self.measured = None; }
}
#[derive(Clone, Copy, Debug)]
struct Track { id: u32, observed: u64, moved: u64, server: Vec3, velocity: Vec3 }
#[derive(Debug)]
pub struct Tracks { slots: [Option<Track>; TRACK_CAPACITY] }
impl Default for Tracks { fn default() -> Self { Self { slots: [None; TRACK_CAPACITY] } } }
#[derive(Clone, Copy, Debug)]
pub struct Motion { pub velocity: Vec3, pub extra_ticks: u32 }
impl Tracks {
    /// Sample once per planning tick, using the last server target, not rendered interpolation.
    /// Reusing an ID after removal requires forget(); login/respawn requires reset().
    pub fn observe(&mut self, id: u32, tick: u64, server: Vec3, interval: u32) -> Result<Motion, Error> {
        if !server.is_finite() || interval == 0 { return Err(Error::Invalid); }
        let at = self.slots.iter().position(|v| v.is_some_and(|t| t.id == id))
            .or_else(|| self.slots.iter().position(Option::is_none)).ok_or(Error::Capacity)?;
        let previous = self.slots[at].filter(|t| tick >= t.observed);
        let mut now = previous.unwrap_or(Track { id, observed: tick, moved: tick,
            server, velocity: Vec3::ZERO });
        if server != now.server {
            let gap = tick.saturating_sub(now.moved).max(u64::from(interval));
            let divisor = if tick.saturating_sub(now.observed) <= 1 { u64::from(interval) } else { gap };
            now.velocity = now.server.vector_to(server).scale(1.0 / divisor as f64);
            now.server = server;
            now.moved = tick;
        }
        now.observed = tick;
        let age = tick.saturating_sub(now.moved);
        let velocity = if age > u64::from(interval) + 1 { Vec3::ZERO } else { now.velocity };
        self.slots[at] = Some(now);
        Ok(Motion { velocity, extra_ticks: age.min(u64::from(interval)) as u32 })
    }
    pub fn forget(&mut self, id: u32) { for slot in &mut self.slots { if slot.is_some_and(|t|t.id==id) {*slot=None;} } }
    pub fn reset(&mut self) { self.slots.fill(None); }
}
pub trait CollisionKernel {
    fn clip(&mut self, bounds: Aabb, attempted: Vec3) -> Result<Vec3, Error>;
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub rendered_feet: Vec3, pub server_feet: Vec3, pub rendered_box: Aabb,
    pub motion: Motion,
    /// Gliding OR touching fluid OR climbing OR no-gravity OR riding.
    pub floating: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Projection { pub offset: Vec3, pub feet: Vec3, pub bounds: Aabb, pub simulated_ticks: u32 }
#[derive(Clone, Copy, Debug, Default)]
pub struct FlightLease { target:Option<u32>, issued:u64 }
impl FlightLease {
    /// The bot owns its airborne-path model. This lease only gates access to it.
    pub fn offer(&mut self,target:u32,local_tick:u64) {self.target=Some(target);self.issued=local_tick;}
    pub fn clear(&mut self) {*self=Self::default();}
    /// Resolve the bot's CURRENT path on demand; do not store offsets that become
    /// stale as the actor moves. Called after observing the ordinary motion track
    /// and before project()'s fast-motion guard. None means ordinary forecasting.
    pub fn offset(&self,target:u32,local_tick:u64,requested:u32,
                  mut path:impl FnMut(u32)->Option<Vec3>)->Option<Vec3> {
        if requested==0 || self.target!=Some(target) || local_tick<self.issued ||
           local_tick-self.issued>2 {return None;}
        path(requested).filter(|p|p.is_finite())
    }
}
pub fn with_offset(snapshot:Snapshot,offset:Vec3)->Result<Projection,Error> {
    let feet=snapshot.rendered_feet.add_vector(offset);
    let bounds=snapshot.rendered_box.move_by(offset);
    if !offset.is_finite() || !feet.is_finite() || !bounds.min_position().is_finite() ||
       !bounds.max_position().is_finite() {return Err(Error::Invalid);}
    Ok(Projection {offset,feet,bounds,simulated_ticks:0})
}
/// `available_steps` is shared across every actor and candidate in one prepare.
/// Cache these projections once per actor/action horizon, not once per blast.
pub fn project(snapshot: Snapshot, requested: u32, available_steps: &mut u32,
               kernel: &mut impl CollisionKernel) -> Result<Projection, Error> {
    let s = snapshot;
    if !s.rendered_feet.is_finite() || !s.server_feet.is_finite() ||
       !s.motion.velocity.is_finite() || !s.rendered_box.min_position().is_finite() ||
       !s.rendered_box.max_position().is_finite() { return Err(Error::Invalid); }
    let lag = s.rendered_feet.vector_to(s.server_feet);
    let mut bounds = s.rendered_box.move_by(lag);
    let mut feet = s.server_feet;
    // Fast untrusted displacements still align with the observed server location.
    let count = if requested == 0 || s.motion.velocity.length_squared() > 1.0 { 0 }
        else { requested.checked_add(s.motion.extra_ticks).ok_or(Error::Budget)? };
    if count > MAX_FORECAST_STEPS || count > *available_steps { return Err(Error::Budget); }
    *available_steps -= count;
    let mut velocity = s.motion.velocity.components();
    for _ in 0..count {
        if !s.floating { velocity[1] -= 0.08; velocity[1] *= 0.98; }
        let attempted = Vec3::new(velocity[0], velocity[1], velocity[2]);
        let actual = kernel.clip(bounds, attempted)?;
        if !actual.is_finite() { return Err(Error::Collision); }
        for (axis, clipped) in actual.components().into_iter().enumerate() {
            if clipped != velocity[axis] { velocity[axis] = 0.0; }
        }
        bounds = bounds.move_by(actual);
        feet = feet.add_vector(actual);
    }
    Ok(Projection { offset: s.rendered_feet.vector_to(feet), feet, bounds, simulated_ticks: count })
}
/// Anchor placement rejects either current or projected other-player occupancy.
pub fn occupies(cell: Aabb, current: Aabb, projected: Projection) -> bool {
    cell.intersects(current) || cell.intersects(projected.bounds)
}
