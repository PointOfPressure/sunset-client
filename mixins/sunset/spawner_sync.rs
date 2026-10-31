fn sunset_spawner_sync(game: &mut crate::game::Game<'_>, pos: (i32, i32, i32), state: horizon_world::registry::BlockStateId) {
    let name = &game.res.facts().get(state).name;
    horizon_client::chunk_map::sunset_spawner_index::state(&mut game.world.chunks, pos, name);
}
