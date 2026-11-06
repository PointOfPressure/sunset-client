// Server Scan: the command tree the server sent, as the chat's tree is built from it (None: a new connection).
fn sunset_server_tree(base: Option<&horizon_client::command::Dispatcher>) {
    crate::game::remote::sunset_server_scan_remote::server_tree(base);
}
