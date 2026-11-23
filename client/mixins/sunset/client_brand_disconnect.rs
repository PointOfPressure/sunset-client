fn sunset_client_brand_disconnect(session: &Session) {
    crate::net::sunset_client_brand::forget(&session.counters);
}
