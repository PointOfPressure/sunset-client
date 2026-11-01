// No Render: a hidden particle is not made at all.
fn sunset_particle_add(options: &ParticleOptions) -> std::ops::ControlFlow<()> {
    if sunset_particles::hidden(options.ty) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
