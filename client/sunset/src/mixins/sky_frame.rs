// Before drawing: the time and weather asked for, as a module may have turned on or off in a tick; also on the title
// screen, where nothing is left of the last world. RF Click learns whether you type; .goto stops or reports; Offhand's
// Auto Totem clicks, between ticks as a screen's clicks are; plugins hear of joins and disconnects; Streamer Mode's
// names to hide (on the title screen too); the chat's suggestions for . commands.
fn sunset_sky_frame(game: &mut crate::game::Game<'_>) {
    crate::game::sunset_packets::register();
    crate::game::sunset_streamer::register();
    crate::game::sunset_atmosphere_renderer::register();
    crate::game::sunset_ui_renderer::begin_frame();
    crate::game::sunset_render::register();
    crate::game::sunset::finish_shutdown(game);
    crate::game::sunset_baritone::maintain(game);
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_gui_move::frame(game);
    crate::game::sunset_antibot::frame(game);
    crate::game::sunset_autoclicker::frame(game);
    crate::game::agent::sunset_goto::frame(game);
    crate::game::sunset_sky::shown(game);
    crate::game::sunset_hands::frame(game);
    crate::game::sunset_combat_inventory::frame(game);
    crate::game::sunset_api::frame(game);
    if !crate::game::sunset::running() { return; }
    crate::game::sunset_streamer_mode::frame(game);
    crate::game::sunset_chat_suggest::frame(game);
}
