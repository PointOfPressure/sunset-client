// Hand View's No equip drop: the hands stay up when the held item changes or the attack cooldown runs, and a new
// item shows at once.
fn sunset_equip_hold(this: &mut HandHeights, changed: [bool; 2]) -> std::ops::ControlFlow<[bool; 2]> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if !crate::first_person::sunset_hand_view::NO_EQUIP_DROP.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Continue(());
    }
    (this.main, this.old_main, this.off, this.old_off) = (1.0, 1.0, 1.0, 1.0);
    std::ops::ControlFlow::Break(changed)
}
