fn sunset_antibot_world() {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_antibot::world_changed();
}
