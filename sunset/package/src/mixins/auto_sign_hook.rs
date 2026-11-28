// Auto Sign (Silent): the editor a server opens for a sign Auto Sign writes is never shown.
fn sunset_auto_sign_open(p: &play::OpenSignEditor) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if crate::game::sunset_auto_sign::swallow(p.pos, p.front) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
