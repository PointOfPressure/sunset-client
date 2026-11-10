// Hand View's Totem pop speed: the pop counts down more ticks each tick (the game's own tick takes the last one).
fn sunset_totem_tick(this: &mut ItemActivation) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    let extra = crate::first_person::sunset_hand_view::TOTEM_EXTRA_TICKS.load(std::sync::atomic::Ordering::Relaxed);
    for _ in 0..extra {
        if this.ticks > 1 {
            this.ticks -= 1;
        }
    }
}
