// No Render: water, lava and powder snow fog turned into plain air.
fn sunset_fog_hook(world: &World, facts: &BlockFacts, camera: [f64; 3], near: &NearPlane) -> std::ops::ControlFlow<FogType> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use std::sync::atomic::Ordering::Relaxed;
    let (water, lava) = (sunset_fog::NO_WATER.load(Relaxed), sunset_fog::NO_LAVA.load(Relaxed));
    if !(water || lava) || sunset_fog::busy() {
        return std::ops::ControlFlow::Continue(());
    }
    sunset_fog::set_busy(true);
    let found = fog_type(world, facts, camera, near);
    sunset_fog::set_busy(false);
    std::ops::ControlFlow::Break(match found {
        FogType::Water if water => FogType::Atmospheric,
        FogType::Lava | FogType::PowderSnow if lava => FogType::Atmospheric,
        other => other,
    })
}
