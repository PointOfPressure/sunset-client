// Anti Vanish keeps a watchlist per server, and Client Spoofer picks the brand for it: the address being joined.
fn sunset_vanish_connect(game: &Game<'_>, address: &str) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    // Joining straight from the command line comes before Sunset's first tick read its settings.
    crate::game::sunset::ensure_loaded(game);
    crate::game::sunset_vanish::connecting(address);
    crate::game::sunset_server_scan::connecting(address);
    crate::game::sunset_sky::forget();
    crate::game::sunset_client_spoofer::connecting(address);
}
