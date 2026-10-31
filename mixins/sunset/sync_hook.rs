// Flight: the next move packet carries the position when the anti kick asks for it. A move packet whose ground Sunset
// changed: the game compares with the ground the server was told.
fn sunset_sync(this: &mut PositionSender) {
    if crate::player_sync::sunset_sync::take() {
        this.reminder = 20;
    }
    if let Some(told) = crate::player_sync::sunset_sync::take_ground() {
        this.on_ground = told;
    }
}
