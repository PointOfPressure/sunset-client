// Hand View's Slow swing: your own swings last longer (the game's own duration otherwise, haste and fatigue too).
fn sunset_swing_hand(this: &mut Game<'_>, hand: Hand, animation: SwingAnimation) -> std::ops::ControlFlow<()> {
    use horizon_client::first_person::sunset_hand_view as view;
    if !view::SLOW_SWING.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Continue(());
    }
    if this.swing.start_if_able(hand, animation, animation.duration.max(view::SLOW_SWING_TICKS)) {
        this.swings_started += 1;
    }
    std::ops::ControlFlow::Break(())
}
