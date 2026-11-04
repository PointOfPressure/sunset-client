// No Render's Hide scoreboard: the sidebar isn't drawn.
fn sunset_scoreboard() -> std::ops::ControlFlow<()> {
    if crate::game::sunset_render::NO_SCOREBOARD.load(std::sync::atomic::Ordering::Relaxed) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
