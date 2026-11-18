fn sunset_antibot_world() {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_antibot::world_changed();
}
