// A chat line starting with `.` is a Meteor command, never sent.
fn meteor_chat(game: &mut crate::game::Game<'_>, message: &str) -> std::ops::ControlFlow<()> {
    if crate::game::meteor::chat(game, message) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
