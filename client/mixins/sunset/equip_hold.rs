// Hand View's No equip drop: the hands stay up when the held item changes or the attack cooldown runs, and a new
// item shows at once. Its 1.8 Animations: the hand does not dip after a hit (1.8 had no cooldown), as Horizon's own
// Combat feel Legacy raises it, but still drops and rises for a new item.
fn sunset_equip_hold(this: &mut HandHeights, hands_busy: bool, swap_scale: f32, changed: [bool; 2]) -> std::ops::ControlFlow<[bool; 2]> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use crate::first_person::sunset_hand_view as view;
    if view::NO_EQUIP_DROP.load(std::sync::atomic::Ordering::Relaxed) {
        (this.main, this.old_main, this.off, this.old_off) = (1.0, 1.0, 1.0, 1.0);
        return std::ops::ControlFlow::Break(changed);
    }
    if view::LEGACY.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break(this.tick_with(hands_busy, swap_scale, changed, true));
    }
    std::ops::ControlFlow::Continue(())
}
