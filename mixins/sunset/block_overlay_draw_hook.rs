// The native depth-tested stroke is skipped only for an owned visual frame.
// No pipeline, buffer, pass, or gameplay state is changed.
fn sunset_block_overlay_draw() -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if sunset_block_overlay_flags::active() {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
