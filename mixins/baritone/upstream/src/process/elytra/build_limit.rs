// baritone.process.elytra.BuildLimitPathFinder: flight above the build limit. The nether pathfinder cannot search above its
// own height, so a long trip climbs out of it, follows a straight line at a fixed level above the world and comes back down
// near the goal. Everything the Java answers asynchronously is answered by a blocking call here; the caller runs it on a
// worker thread.
use super::baritone_elytra_segment::{PathCalculationError, UnpackedSegment};

type Pos = [i32; 3];

/// The nether pathfinder behind it: `NetherPathfinderContext.pathFindAsync`, blocking.
pub trait NetherSearch {
    fn path_find(&self, src: Pos, dst: Pos) -> Result<UnpackedSegment, PathCalculationError>;
}

/// What it needs to know of the world.
pub trait SkyProbe {
    /// Whether the game has chunk (x, z) loaded.
    fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool;
    /// `getHeight(Heightmap.Types.MOTION_BLOCKING, x, z)`.
    fn motion_blocking_height(&self, x: i32, z: i32) -> i32;
}

pub struct BuildLimitPathFinder<'a> {
    /// `ctx.world().getMaxY() + 16`.
    flight_level: i32,
    min_y: i32,
    max_y: i32,
    /// `netherCtx.getMaxHeight()`.
    nether_max_height: i32,
    /// `elytraLongDistanceThreshold`.
    long_distance_threshold: i64,
    nether: &'a dyn NetherSearch,
    sky: &'a dyn SkyProbe,
}

/// Java's `Math.round` for a double: halves go up.
fn java_round(x: f64) -> i32 {
    (x + 0.5).floor() as i32
}

/// `ChunkPos.getMiddleBlockPosition`.
fn middle_block(chunk_x: i32, y: i32, chunk_z: i32) -> Pos {
    [(chunk_x << 4) + 8, y, (chunk_z << 4) + 8]
}

impl<'a> BuildLimitPathFinder<'a> {
    /// `min_y` and `max_y` are the lowest and highest block of the world (`getMinY`, `getMaxY`).
    pub fn new(min_y: i32, max_y: i32, nether_max_height: i32, long_distance_threshold: i64, nether: &'a dyn NetherSearch, sky: &'a dyn SkyProbe) -> Result<Self, String> {
        if nether_max_height + min_y < max_y {
            return Err("Nether pathfinder max height is below world build limit, cannot proceed".into());
        }
        Ok(Self { flight_level: max_y + 16, min_y, max_y, nether_max_height, long_distance_threshold, nether, sky })
    }

    fn nether_top(&self) -> i32 {
        self.nether_max_height + self.min_y - 1
    }

    /// Generates a direct path from the start to the destination at a fixed y-level above the build limit. `buffer_distance`
    /// is the distance from the destination to halt the direct path, `max_path_size` the number of nodes at most. The path,
    /// and whether it is complete.
    pub fn generate_direct_path(&self, start: Pos, destination: Pos, buffer_distance: i32, max_path_size: usize) -> (Vec<Pos>, bool) {
        let mut path: Vec<Pos> = Vec::new();
        let step_distance = 32.0;

        let start_fixed = if start[1] == self.flight_level { start } else { [start[0], self.flight_level, start[2]] };
        let destination_fixed = if destination[1] == self.flight_level { destination } else { [destination[0], self.flight_level, destination[2]] };

        let mut cur = start_fixed;
        path.push(cur);

        while path.len() < max_path_size {
            let delta_x = f64::from(destination_fixed[0] - cur[0]);
            let delta_z = f64::from(destination_fixed[2] - cur[2]);
            let remaining_distance = (delta_x * delta_x + delta_z * delta_z).sqrt();
            let remaining_distance_sq = delta_x * delta_x + delta_z * delta_z;

            // The Java multiplies ints, which wrap.
            if remaining_distance_sq <= f64::from(buffer_distance.wrapping_mul(buffer_distance)) {
                // We are within the buffer distance, so we can stop here.
                return (path, true);
            } else if remaining_distance <= step_distance {
                path.push(destination_fixed);
                return (path, true);
            }

            let step_ratio = step_distance / remaining_distance;
            let next_x = java_round(f64::from(cur[0]) + delta_x * step_ratio);
            let next_z = java_round(f64::from(cur[2]) + delta_z * step_ratio);

            cur = [next_x, self.flight_level, next_z];
            path.push(cur);
        }

        (path, false)
    }

    /// Attempts to find an open spot in the sky to transition up above the build limit so a simple direct path can be
    /// followed. The path, and whether a transition was found.
    pub fn generate_transition_up(&self, start: Pos, destination: Pos) -> (Vec<Pos>, bool) {
        let delta_x = f64::from(destination[0] - start[0]);
        let delta_z = f64::from(destination[2] - start[2]);
        let distance = (delta_x * delta_x + delta_z * delta_z).sqrt();

        let scale = 8.0 / distance;
        let step_x = delta_x * scale;
        let step_z = delta_z * scale;

        let nether_max_height = self.nether_top();
        let (start_chunk_x, start_chunk_z) = (start[0] >> 4, start[2] >> 4);

        if !self.is_sky_clear(start_chunk_x, start_chunk_z, start[1]) {
            return (Vec::new(), false);
        }

        // Start with the middle block so the transition doesn't leave the only chunk we can confirm is clear.
        let middle = middle_block(start_chunk_x, nether_max_height + 4, start_chunk_z);

        let i = 2;
        let next = [(f64::from(middle[0]) + step_x * f64::from(i)) as i32, nether_max_height + i * 8, (f64::from(middle[2]) + step_z * f64::from(i)) as i32];
        (vec![next], true)
    }

    /// Attempts to find an open spot in the sky to transition down to a flight level the nether pathfinder can navigate at.
    /// The path (a single point), and whether a transition point was found.
    pub fn generate_transition_down(&self, start: Pos) -> (Vec<Pos>, bool) {
        let nether_max_height = self.nether_top();
        let (start_chunk_x, start_chunk_z) = (start[0] >> 4, start[2] >> 4);

        if !self.is_sky_clear(start_chunk_x, start_chunk_z, nether_max_height - 16) {
            return (Vec::new(), false);
        }

        (vec![middle_block(start_chunk_x, nether_max_height - 8, start_chunk_z)], true)
    }

    pub fn is_sky_clear(&self, chunk_x: i32, chunk_z: i32, y: i32) -> bool {
        if !self.sky.has_chunk(chunk_x, chunk_z) {
            return false;
        }
        for x in 0..16 {
            for z in 0..16 {
                let height = self.sky.motion_blocking_height((chunk_x << 4) + x, (chunk_z << 4) + z);
                if height > y {
                    return false;
                }
            }
        }
        true
    }

    /// `pathFindAsync`, blocking.
    pub fn path_find(&self, src: Pos, dst: Pos) -> Result<UnpackedSegment, PathCalculationError> {
        let nether_max_height = self.nether_top();
        let max_direct_path_size = 500;

        // There can be some navigation issues around failed transitions if the threshold distance isn't large enough.
        let max_distance = if self.long_distance_threshold >= 32 { self.long_distance_threshold as f64 } else { f64::INFINITY };

        let (dx, dz) = (f64::from(src[0] - dst[0]), f64::from(src[2] - dst[2]));
        let distance_xz = dx * dx + dz * dz;
        let is_long_distance = distance_xz > max_distance * max_distance;
        let src_above_supported_height = src[1] >= nether_max_height;
        let dst_above_supported_height = dst[1] >= nether_max_height;

        if src_above_supported_height && dst_above_supported_height {
            let (path, finished) = self.generate_direct_path(src, dst, 0, max_direct_path_size);
            return Ok(UnpackedSegment::new(path, finished));
        }

        if is_long_distance {
            if src_above_supported_height {
                let (path, finished) = self.generate_direct_path(src, dst, max_distance as i32, max_direct_path_size);
                return Ok(UnpackedSegment::new(path, dst_above_supported_height && finished));
            }
            let (mut path, success) = self.generate_transition_up(src, dst);
            if success {
                let from = *path.last().expect("a transition has a point");
                let (direct, finished) = self.generate_direct_path(from, dst, max_distance as i32, max_direct_path_size);
                path.extend(direct);
                return Ok(UnpackedSegment::new(path, dst_above_supported_height && finished));
            }

            // Failed to find a transition point so navigate a bit in the right direction and try.
            let delta_x = f64::from(dst[0] - src[0]);
            let delta_z = f64::from(dst[2] - src[2]);
            let scale = (max_distance / 2.0) / (delta_x * delta_x + delta_z * delta_z).sqrt();
            let step_x = delta_x * scale;
            let step_z = delta_z * scale;
            let mid_dst = [(f64::from(src[0]) + step_x) as i32, nether_max_height, (f64::from(src[2]) + step_z) as i32];

            // A wrapper for a nether pathfinder call but the returned path will always indicate it is incomplete.
            let packed = self.nether.path_find(src, mid_dst)?;
            return Ok(UnpackedSegment::new(packed.collect(), false));
        }

        if src_above_supported_height {
            let (mut path, success) = self.generate_transition_down(src);

            if !success {
                let new_dest = if distance_xz > 32.0 { dst } else { [dst[0], self.max_y, dst[2]] };
                let (direct, finished) = self.generate_direct_path(src, new_dest, 0, 2);
                return Ok(UnpackedSegment::new(direct, finished));
            }

            let from = *path.last().expect("a transition has a point");
            let np = self.nether.path_find(from, dst)?;
            path.extend(np.collect());
            return Ok(UnpackedSegment::new(path, np.is_finished()));
        }

        if dst_above_supported_height {
            let (mut path, success) = self.generate_transition_up(src, dst);

            if success {
                let from = *path.last().expect("a transition has a point");
                let (direct, finished) = self.generate_direct_path(from, dst, 0, max_direct_path_size);
                path.extend(direct);
                return Ok(UnpackedSegment::new(path, finished));
            }

            return self.nether.path_find(src, [dst[0], nether_max_height, dst[2]]);
        }

        self.nether.path_find(src, dst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Answers every search with a straight line of points and remembers what it was asked.
    struct Straight {
        asked: RefCell<Vec<(Pos, Pos)>>,
    }

    impl NetherSearch for Straight {
        fn path_find(&self, src: Pos, dst: Pos) -> Result<UnpackedSegment, PathCalculationError> {
            self.asked.borrow_mut().push((src, dst));
            Ok(UnpackedSegment::new(vec![src, dst], true))
        }
    }

    struct Sky {
        loaded: bool,
        height: i32,
    }

    impl SkyProbe for Sky {
        fn has_chunk(&self, _: i32, _: i32) -> bool {
            self.loaded
        }
        fn motion_blocking_height(&self, _: i32, _: i32) -> i32 {
            self.height
        }
    }

    // An overworld: blocks -64..=319, the nether pathfinder able to reach y 320 (384 - 64).
    fn finder<'a>(n: &'a Straight, s: &'a Sky) -> BuildLimitPathFinder<'a> {
        BuildLimitPathFinder::new(-64, 319, 384, 500, n, s).unwrap()
    }

    #[test]
    fn a_direct_path_climbs_to_the_flight_level_and_steps_32_blocks_at_a_time() {
        let n = Straight { asked: RefCell::new(vec![]) };
        let s = Sky { loaded: true, height: 70 };
        let f = finder(&n, &s);
        let (path, done) = f.generate_direct_path([0, 100, 0], [200, 100, 0], 0, 500);
        assert!(done);
        assert_eq!(path[0], [0, 335, 0]);
        assert_eq!(path[1], [32, 335, 0]);
        assert_eq!(path.last().unwrap(), &[200, 335, 0]);
        // Within the buffer distance it stops early.
        let (short, done) = f.generate_direct_path([0, 100, 0], [200, 100, 0], 100, 500);
        assert!(done && short.last().unwrap()[0] >= 100 && short.last().unwrap()[0] < 200);
        // A path size limit makes it incomplete.
        let (cut, done) = f.generate_direct_path([0, 100, 0], [2000, 100, 0], 0, 5);
        assert!(!done);
        assert_eq!(cut.len(), 5);
    }

    #[test]
    fn rounding_follows_java_for_negative_halves() {
        assert_eq!(java_round(-2.5), -2);
        assert_eq!(java_round(2.5), 3);
        assert_eq!(java_round(-2.6), -3);
    }

    #[test]
    fn a_trip_that_stays_below_the_roof_is_the_nether_pathfinders() {
        let n = Straight { asked: RefCell::new(vec![]) };
        let s = Sky { loaded: true, height: 70 };
        let r = finder(&n, &s).path_find([0, 80, 0], [100, 80, 0]).unwrap();
        assert!(r.is_finished());
        assert_eq!(n.asked.borrow().as_slice(), &[([0, 80, 0], [100, 80, 0])]);
    }

    #[test]
    fn a_long_trip_climbs_out_flies_the_line_and_is_not_finished_unless_the_goal_is_up_there() {
        let n = Straight { asked: RefCell::new(vec![]) };
        let s = Sky { loaded: true, height: 70 };
        let r = finder(&n, &s).path_find([0, 80, 0], [5000, 80, 0]).unwrap();
        // The first point is the transition above the nether pathfinder's reach.
        assert!(r.collect()[0][1] > 319);
        assert!(!r.is_finished(), "the goal is below the build limit, so it is a segment");
        assert!(n.asked.borrow().is_empty(), "no nether search was needed");
    }

    #[test]
    fn a_failed_transition_navigates_a_bit_in_the_right_direction_instead() {
        let n = Straight { asked: RefCell::new(vec![]) };
        // The sky above the start chunk is not clear.
        let s = Sky { loaded: true, height: 400 };
        let r = finder(&n, &s).path_find([0, 80, 0], [5000, 80, 0]).unwrap();
        assert!(!r.is_finished());
        let asked = n.asked.borrow();
        assert_eq!(asked.len(), 1);
        // Half the threshold along the way, at the top of the nether pathfinder's reach.
        assert_eq!(asked[0].1, [250, 319, 0]);
    }

    #[test]
    fn coming_down_near_the_goal_goes_through_the_middle_of_the_chunk() {
        let n = Straight { asked: RefCell::new(vec![]) };
        let s = Sky { loaded: true, height: 70 };
        let r = finder(&n, &s).path_find([5, 335, 5], [60, 80, 5]).unwrap();
        let path = r.collect();
        assert_eq!(path[0], [8, 311, 8]);
        assert_eq!(n.asked.borrow()[0].0, [8, 311, 8]);
    }

    #[test]
    fn a_nether_pathfinder_that_cannot_reach_the_build_limit_is_refused() {
        let n = Straight { asked: RefCell::new(vec![]) };
        let s = Sky { loaded: true, height: 70 };
        // Height 128 reaches only y 63 from -64.
        assert!(BuildLimitPathFinder::new(-64, 319, 128, 500, &n, &s).is_err());
    }
}
