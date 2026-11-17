// A native screen transition releases/restores latches without mouse events.
fn sunset_input_screen(value: (), game: &crate::game::Game<'_>) {
    let _ = value;
    crate::game::remote::sunset_combat_remote::input_screen(game);
}
