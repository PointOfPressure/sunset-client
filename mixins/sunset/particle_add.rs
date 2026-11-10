// No Render: a hidden particle is not made at all.
fn sunset_particle_add(options: &ParticleOptions) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if sunset_particles::hidden(options.ty) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
