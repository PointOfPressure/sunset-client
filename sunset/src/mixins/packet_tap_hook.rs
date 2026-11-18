// Before the game takes in what the server sent: the new events are taken in here first, through Sunset
// (crate::game::sunset_packets::poll: copies for Crystal Aura, Anchor Aura and the plugin API, what Anti trap drops, what
// Backtrack and Knockback Delay hold with their receive credit, GUI Move's held inventory updates and Client Spoofer's
// watch for the Login packets its join packets follow), and what is left joins the game's backlog in order. The game's
// own take a moment later finds only what arrived in between.
fn sunset_packet_tap(game: &mut Game<'_>) {
    let Some(r) = game.remote.as_mut() else {
        return;
    };
    let mut fresh = std::collections::VecDeque::new();
    r.receive_credited_backlog(&mut fresh);
    crate::game::sunset_packets::poll(&r.session, &mut fresh, 0);
    r.backlog.extend(fresh);
}
