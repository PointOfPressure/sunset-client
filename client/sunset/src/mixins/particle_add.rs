// No Render: a hidden particle is not made at all.
fn sunset_particle_add(options: &ParticleOptions) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if ::sunset_core::particles::any() && ::sunset_core::particles::hidden(options.ty.name()) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
