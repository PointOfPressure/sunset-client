// A chat line starting with `.` is a Sunset command, never sent.
fn sunset_chat(game: &mut crate::game::Game<'_>, message: &str) -> std::ops::ControlFlow<()> {
    if crate::game::sunset::chat(game, message) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
