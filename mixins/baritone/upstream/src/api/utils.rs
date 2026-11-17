// baritone.api.utils: positions, directions and the yaw maths every other part uses. Plain Rust, no
// game types, so it is tested on its own.

/// A block position: x, y, z.
pub type Pos = [i32; 3];

pub fn up(p: Pos, n: i32) -> Pos {
    [p[0], p[1] + n, p[2]]
}

pub fn dist_sq(a: Pos, b: Pos) -> i64 {
    let d = [i64::from(a[0] - b[0]), i64::from(a[1] - b[1]), i64::from(a[2] - b[2])];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// `IPlayerContext.playerFeet` before its slab rule (`baritone_gworld::player_feet` is the whole of it): a little
/// above the position, because soul sand, mud and farmland are lower than a full block.
pub fn feet_of(p: [f64; 3]) -> Pos {
    [p[0].floor() as i32, (p[1] + 0.1251).floor() as i32, p[2].floor() as i32]
}

/// `Entity.blockPosition`: the block a position is in.
pub fn block_pos_of(p: [f64; 3]) -> Pos {
    [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32]
}

/// The cardinal direction a yaw faces, as (dx, dz).
pub fn facing_step(yaw: f32) -> (i32, i32) {
    let r = f64::from(yaw).to_radians();
    let (x, z) = (-r.sin(), r.cos());
    if x.abs() > z.abs() { (x.signum() as i32, 0) } else { (0, z.signum() as i32) }
}

/// A direction by name, as a unit vector: down, up, north (-z), south (+z), west (-x), east (+x).
pub fn direction_vector(name: &str) -> Option<[i32; 3]> {
    match name.to_ascii_lowercase().as_str() {
        "down" => Some([0, -1, 0]),
        "up" => Some([0, 1, 0]),
        "north" => Some([0, 0, -1]),
        "south" => Some([0, 0, 1]),
        "west" => Some([-1, 0, 0]),
        "east" => Some([1, 0, 0]),
        _ => None,
    }
}

thread_local! {
    /// `censorCoordinates`, kept where the code that prints a coordinate can reach it.
    static CENSOR: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_censor(on: bool) {
    CENSOR.with(|c| c.set(on));
}

/// `SettingsUtil.maybeCensor`: a coordinate as text, or `<censored>`.
pub fn maybe_censor(coordinate: i32) -> String {
    if CENSOR.with(std::cell::Cell::get) { "<censored>".to_owned() } else { coordinate.to_string() }
}

/// A block position as Baritone prints it.
pub fn pos_label(p: Pos) -> String {
    format!("BetterBlockPos{{x={},y={},z={}}}", maybe_censor(p[0]), maybe_censor(p[1]), maybe_censor(p[2]))
}

#[cfg(test)]
mod censor_tests {
    use super::*;

    #[test]
    fn coordinates_are_hidden_while_the_setting_is_on() {
        set_censor(true);
        assert_eq!(pos_label([1, 2, 3]), "BetterBlockPos{x=<censored>,y=<censored>,z=<censored>}");
        set_censor(false);
        assert_eq!(pos_label([1, 2, 3]), "BetterBlockPos{x=1,y=2,z=3}");
    }
}

/// The column a distance away along a yaw (GoalXZ.fromDirection): 0 faces south, 90 west.
pub fn column_ahead(from: [f64; 3], yaw: f64, distance: f64) -> (i32, i32) {
    let r = yaw.to_radians();
    ((from[0] - r.sin() * distance).floor() as i32, (from[2] + r.cos() * distance).floor() as i32)
}
