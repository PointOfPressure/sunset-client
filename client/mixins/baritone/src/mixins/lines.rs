// The path, what it breaks and places, and the goal: added to the frame's gizmos, which the game turns into what the
// GPU draws (depth-tested lines, and the lines drawn over everything).
fn baritone_lines(mut value: Gizmos) -> Gizmos {
    crate::game::baritone_lines::add(&mut value);
    value
}
