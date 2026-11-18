// Hand View's No equip drop: the hands stay up when the held item changes or the attack cooldown runs, and a new
// item shows at once.
fn sunset_equip_hold(value: [bool; 2], game: &mut Game<'_>, changed: [bool; 2]) -> [bool; 2] {
    if ::sunset_core::lifecycle::RUNTIME.running()
        && ::sunset_core::hand_view::NO_EQUIP_DROP.load(std::sync::atomic::Ordering::Relaxed)
    {
        let h = &mut game.heights;
        (h.main, h.old_main, h.off, h.old_off) = (1.0, 1.0, 1.0, 1.0);
        return changed;
    }
    value
}
