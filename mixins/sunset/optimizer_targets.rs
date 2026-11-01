// Invoke the original native body once under a per-thread reentry guard. The
// pinned loader cannot transform returns in a function containing closures.
// No native target enumeration or stable special-id mapping is duplicated.
fn sunset_optimizer_targets(game:&crate::game::Game<'_>)
    ->std::ops::ControlFlow<Vec<(u32,Aabb)>> {
    thread_local! {static SUNSET_TARGET_PASS:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};}
    if SUNSET_TARGET_PASS.with(|flag|flag.get())
        || !crate::game::sunset_crystal_optimizer::has_predictions(game) {
        return std::ops::ControlFlow::Continue(());
    }
    struct Reset;
    impl Drop for Reset {fn drop(&mut self){SUNSET_TARGET_PASS.with(|flag|flag.set(false));}}
    SUNSET_TARGET_PASS.with(|flag|flag.set(true));
    let _reset=Reset;
    let mut targets=game.special_targets();
    targets.retain(|(id,_)| id.checked_sub(CRYSTAL_IDS)
        .and_then(|index|game.crystals.get(index as usize))
        .is_none_or(|crystal| !crate::game::sunset_crystal_optimizer::hidden_position(
            game,[crystal.position.x,crystal.position.y,crystal.position.z])));
    std::ops::ControlFlow::Break(targets)
}
