// Refresh only the active native prediction immediately before queued native input attacks.
fn sunset_optimizer_before_attack(game:&mut crate::game::Game<'_>) {
    if !game.native_window() || !game.native_on(crate::game::sunset::ID)
        || !crate::game::sunset::with(|s|s.module("Crystal Optimizer").is_some_and(|m|m.on)) {return;}
    let tick=game.ticks;
    crate::game::sunset_crystal_optimizer::tick(game,tick);
}
