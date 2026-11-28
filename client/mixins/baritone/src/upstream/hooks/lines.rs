// The path, what it breaks and places, and the goal, drawn over the world.
fn baritone_lines(
    lines: &[crate::gizmos::Line],
    camera: [f64; 3],
    view: glam::Mat4,
) -> core::ops::ControlFlow<Vec<[f32; 8]>> {
    crate::gizmos::baritone_lines::take_over(lines, camera, view)
}
