// Hand View's 1.8 Animations: the hand does not dip after a hit (1.8 had no cooldown), as Horizon's own Combat feel
// Legacy raises it, but still drops and rises for a new item.
fn sunset_equip_legacy(value: bool) -> bool {
    value
        || (::sunset_core::lifecycle::RUNTIME.running()
            && ::sunset_core::hand_view::LEGACY.load(std::sync::atomic::Ordering::Relaxed))
}
