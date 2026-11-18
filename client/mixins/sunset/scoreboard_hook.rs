// No Render's Hide scoreboard: the sidebar isn't drawn.
fn sunset_scoreboard() -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if crate::game::sunset_render::NO_SCOREBOARD.load(std::sync::atomic::Ordering::Relaxed) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
