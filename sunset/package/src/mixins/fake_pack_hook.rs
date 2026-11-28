// Client Spoofer's Fake resource packs: the pushed pack is answered as taken, and not downloaded, applied or asked
// about.
fn sunset_fake_pack(game: &mut Game<'_>, p: &ResourcePackPush) -> ::core::ops::ControlFlow<()> {
    if ::sunset_core::lifecycle::RUNTIME.running()
        && valid_url(&p.url)
        && crate::game::sunset_client_spoofer::fake_pack(game, p.id)
    {
        return ::core::ops::ControlFlow::Break(());
    }
    ::core::ops::ControlFlow::Continue(())
}
