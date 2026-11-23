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
    // The game stamps each event with the time it took it in (its backlog's BACKLOG_LIMIT counts from it). What is
    // new here was all taken in now; what Sunset held before and lets go now joins with that same time.
    let now = fresh.front().map_or_else(std::time::Instant::now, |(_, at)| *at);
    let mut items: std::collections::VecDeque<_> = fresh.into_iter().map(|(item, _)| item).collect();
    crate::game::sunset_packets::poll(&r.session, &mut items, 0);
    if r.backlog.is_empty() && !items.is_empty() {
        r.backlog_since = Some(now);
    }
    r.backlog.extend(items.into_iter().map(|item| (item, now)));
}
