// GUI Move: behind a screen the sprint phase sees the forward key GUI Move walks you by, so a sprint goes on (the
// call again has the forward key, so it passes through here).
fn sunset_screen_sprint(player: &mut PlayerMotion, keys: crate::sprint::Keys, window: i32) -> std::ops::ControlFlow<Result<()>> {
    match crate::motion::sunset_motion::screen_sprint_keys(keys) {
        Some(keys) => std::ops::ControlFlow::Break(player.sprint_input_tick_with_window(keys, window)),
        None => std::ops::ControlFlow::Continue(()),
    }
}
