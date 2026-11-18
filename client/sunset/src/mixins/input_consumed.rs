// The native input pass consumed its queue, including Sunset's contributions.
fn sunset_input_consumed(value: ()) {
    let _ = value;
    crate::game::remote::sunset_combat_remote::input_consumed();
}
