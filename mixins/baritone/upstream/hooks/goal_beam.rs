fn baritone_goal_beam(game: &mut crate::game::Game<'_>, camera: glam::DVec3, alpha: f64, out: &mut Vec<horizon_client::mesher::Vertex>) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    crate::game::baritone::goal_beams(game, camera, alpha, out);
}
