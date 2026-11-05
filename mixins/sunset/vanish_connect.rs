// Anti Vanish keeps a watchlist per server, and Client Spoofer picks the brand for it: the address being joined.
fn sunset_vanish_connect(address: &str) {
    crate::game::sunset_vanish::connecting(address);
    crate::game::sunset_server_scan::connecting(address);
    crate::game::sunset_sky::forget();
    crate::game::sunset_client_spoofer::connecting(address);
}
