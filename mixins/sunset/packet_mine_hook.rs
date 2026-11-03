// Packet Mine: the button held on a block queues it instead of mining it the game's way. No Interact: held on what
// it stops, nothing.
fn sunset_packet_mine_held(game: &mut crate::game::Game<'_>, down: bool) -> std::ops::ControlFlow<()> {
    if down && (crate::game::sunset_parity::hit_blocked(game) || crate::game::sunset_packet_mine::click(game, false)) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
