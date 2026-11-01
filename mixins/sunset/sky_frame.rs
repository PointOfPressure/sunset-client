// Before drawing: the time and weather asked for, as a module may have turned on or off in a tick; also on the title
// screen, where nothing is left of the last world. RF Click learns whether you type.
fn sunset_sky_frame(game: &mut crate::game::Game<'_>) {
    crate::game::sunset_sky::shown(game);
    crate::game::sunset_hands::frame(game);
}
