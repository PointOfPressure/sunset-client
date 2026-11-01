// No Render: a hidden particle made outside the options path is not kept.
fn sunset_particle_push(particle: &Particle) -> std::ops::ControlFlow<()> {
    if sunset_particles::hidden(particle.ty) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
