// Anti Vanish keeps a watchlist per server: the address being joined.
fn sunset_vanish_connect(address: &str) {
    crate::game::sunset_vanish::connecting(address);
    crate::game::sunset_sky::forget();
}
