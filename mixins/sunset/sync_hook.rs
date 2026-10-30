// Flight: the next move packet carries the position when the anti kick asks for it.
fn sunset_sync(this: &mut PositionSender) {
    if crate::player_sync::sunset_sync::take() {
        this.reminder = 20;
    }
}
