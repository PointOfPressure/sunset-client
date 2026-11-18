// No Render: a hidden particle the game makes outside the options path (block crumbs, cracks, the warden's digging)
// is gone at once and never seen.
fn sunset_particle_push(value: horizon_client::particles::Particle) -> horizon_client::particles::Particle {
    let mut particle = value;
    if ::sunset_core::lifecycle::RUNTIME.running()
        && ::sunset_core::particles::any()
        && ::sunset_core::particles::hidden(particle.ty.name())
    {
        particle.age = particle.lifetime;
        particle.size = 0.0;
        particle.rgba[3] = 0.0;
    }
    particle
}
