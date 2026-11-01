// The native body runs once under reentry. HEAD binds the original i32 id
// before native code shadows it; no RETURN binding casts are required.
fn sunset_restore_pearl_owner(
    game: &mut crate::game::Game<'_>,
    id: i32,
    kind: &str,
    position: [f64; 3],
    rotation: (f32, f32),
    velocity: [f64; 3],
    data: i32,
) -> std::ops::ControlFlow<bool> {
    if kind != "minecraft:ender_pearl"
        || !game.native_window()
        || !game.native_on(crate::game::sunset::ID)
    {
        return std::ops::ControlFlow::Continue(());
    }
    thread_local! {
        static SUNSET_PEARL_SPAWN_PASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if SUNSET_PEARL_SPAWN_PASS.with(|flag| flag.get()) {
        return std::ops::ControlFlow::Continue(());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SUNSET_PEARL_SPAWN_PASS.with(|flag| flag.set(false));
        }
    }
    SUNSET_PEARL_SPAWN_PASS.with(|flag| flag.set(true));
    let _reset = Reset;
    let accepted = game.spawn_remote_projectile(id, kind, position, rotation, velocity, data);
    std::ops::ControlFlow::Break(crate::game::sunset_pearl_owner::accepted_spawn(
        game, accepted, id, kind, data,
    ))
}
