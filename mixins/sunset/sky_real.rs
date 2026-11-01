// Before the frame's packets: the world holds the real time and weather while the server writes to them.
fn sunset_sky_real(game: &mut crate::game::Game<'_>) {
    crate::game::sunset_sky::real(game);
}
