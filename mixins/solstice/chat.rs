// A chat line starting with `.` is a Solstice command, never sent.
fn solstice_chat(game: &mut crate::game::Game<'_>, message: &str) -> std::ops::ControlFlow<()> {
    if crate::game::solstice::chat(game, message) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
