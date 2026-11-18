// No Render: a hidden particle the game makes outside the options path (block crumbs and breaking, cracks, the
// warden's digging, the renderer's own) is never added.
fn sunset_particle_push(particle: &Particle) -> std::ops::ControlFlow<()> {
    if ::sunset_core::lifecycle::RUNTIME.running()
        && ::sunset_core::particles::any()
        && ::sunset_core::particles::hidden(particle.ty.name())
    {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
