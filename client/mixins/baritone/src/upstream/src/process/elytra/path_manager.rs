// baritone.process.elytra.ElytraBehavior.PathManager: the flight path, how far along it the player is, and when to search
// again: around something in the way, when it stops getting anywhere, and for the next stretch once the player nears the
// end of what is known. The Java starts a CompletableFuture for each search and chains what to do with the answer onto it;
// here `tick` returns a `Job` to run (on a worker thread, with `run_job` or the glue's own), and `finish` applies the
// answer. Only one job runs at a time, as `recalculating` says.
use super::baritone_elytra_flight::{FlightWorld, V3};
use super::baritone_elytra_segment::{NetherPath, PathCalculationError, UnpackedSegment};
use std::time::Instant;

type Pos = [i32; 3];

fn dist_sq(a: Pos, b: Pos) -> f64 {
    let (dx, dy, dz) = (i64::from(a[0] - b[0]), i64::from(a[1] - b[1]), i64::from(a[2] - b[2]));
    (dx * dx + dy * dy + dz * dz) as f64
}

fn dist(a: Pos, b: Pos) -> f64 {
    dist_sq(a, b).sqrt()
}

/// `BlockPos.toString()`.
pub fn block_pos_text(p: Pos) -> String {
    format!("BlockPos{{x={}, y={}, z={}}}", p[0], p[1], p[2])
}

/// What the manager reads of the game, beyond the world a flight is solved in.
pub trait PathEnv: FlightWorld {
    fn player_feet(&self) -> Pos;
    /// `ctx.player().getY()`.
    fn player_y(&self) -> f64;
    fn is_nether(&self) -> bool;
    /// `ctx.playerFeetAsVec()`.
    fn player_feet_vec(&self) -> V3;
    fn player_head(&self) -> V3;
    fn is_fall_flying(&self) -> bool;
    /// `npfContext.hasChunk`: the pathfinder holds the chunk, from the game.
    fn npf_has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool;
    /// `ctx.world().getChunkSource().hasChunk`.
    fn world_has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool;
    fn destination_fixed(&self) -> Pos;
    /// `ElytraProcess.State.LANDING`.
    fn is_landing(&self) -> bool;
    /// `ElytraProcess.State.GET_TO_JUMP`.
    fn is_get_to_jump(&self) -> bool;
    /// `ctx.player().distanceToSqr(pos.getCenter())`.
    fn player_distance_sqr_to_center(&self, pos: Pos) -> f64;
}

/// What a chat line is: `logDirect` shows always, `logVerbose` only with elytraChatSpam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    Direct(String),
    Verbose(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobKind {
    Destination,
    RecalcSegment,
    NextSegment { path_start: Pos },
}

/// How the answer is joined to the path that was already there (the `UnaryOperator` of `path0`).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Join {
    Identity,
    Append { after: Vec<Pos>, complete: bool, up_to_given: bool },
    Prepend { before: Vec<Pos> },
}

/// One search to run: from `src` towards `dst`.
pub struct Job {
    pub src: Pos,
    pub dst: Pos,
    kind: JobKind,
    join: Join,
    started: Instant,
    /// Said when it worked, `{secs}` being the time it took.
    note_on_success: Option<String>,
    /// For the distance in "Computed path": the number of blocks the path had before this job's answer was put in front.
    before_len: usize,
}

impl Job {
    pub fn kind(&self) -> &JobKind {
        &self.kind
    }
}

/// What `finish` found out that the process has to act on.
#[derive(Default)]
pub struct Finished {
    /// `process.landingSpotIsBad`.
    pub bad_landing_spot: Option<Pos>,
}

pub struct PathManager {
    path: NetherPath,
    complete_path: bool,
    recalculating: bool,
    max_player_near: usize,
    ticks_near_unchanged: i32,
    player_near: usize,
    append_destination: bool,
    notes: Vec<Note>,
}

impl PathManager {
    pub fn new(append_destination: bool) -> Self {
        Self {
            path: NetherPath::empty(),
            complete_path: true,
            recalculating: false,
            max_player_near: 0,
            ticks_near_unchanged: 0,
            player_near: 0,
            append_destination,
            notes: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.path = NetherPath::empty();
        self.complete_path = true;
        self.recalculating = false;
        self.player_near = 0;
        self.ticks_near_unchanged = 0;
        self.max_player_near = 0;
    }

    pub fn path(&self) -> &NetherPath {
        &self.path
    }

    pub fn near(&self) -> usize {
        self.player_near
    }

    pub fn is_complete(&self) -> bool {
        self.complete_path
    }

    #[allow(dead_code)]
    pub fn is_recalculating(&self) -> bool {
        self.recalculating
    }

    /// The chat lines said since the last call.
    pub fn take_notes(&mut self) -> Vec<Note> {
        std::mem::take(&mut self.notes)
    }

    fn direct(&mut self, s: String) {
        self.notes.push(Note::Direct(s));
    }

    fn verbose(&mut self, s: String) {
        self.notes.push(Note::Verbose(s));
    }

    pub fn tick(&mut self, env: &dyn PathEnv) -> Option<Job> {
        // Recalculate the closest path node.
        self.update_player_near(env.player_feet());
        let prev_max_near = self.max_player_near;
        self.max_player_near = self.max_player_near.max(self.player_near);

        if self.max_player_near == prev_max_near && env.is_fall_flying() {
            self.ticks_near_unchanged += 1;
        } else {
            self.ticks_near_unchanged = 0;
        }

        // Obstacles are more important than an incomplete path, handle those first.
        let job = self.pathfind_around_obstacles(env);
        if job.is_some() {
            return job;
        }
        self.attempt_next_segment(env)
    }

    /// `pathToDestination`.
    pub fn path_to_destination(&mut self, from: Pos, env: &dyn PathEnv) -> Job {
        self.job(from, env.destination_fixed(), JobKind::Destination, Join::Identity, None)
    }

    fn job(&self, src: Pos, dst: Pos, kind: JobKind, join: Join, note_on_success: Option<String>) -> Job {
        Job { src, dst, kind, join, started: Instant::now(), note_on_success, before_len: 0 }
    }

    /// `pathRecalcSegment`: searches again from the player to the node `up_to_incl`, or to the destination, keeping the
    /// path after that node.
    pub fn path_recalc_segment(&mut self, up_to_incl: Option<usize>, env: &dyn PathEnv, note: Option<String>) -> Job {
        assert!(!self.recalculating, "already recalculating");
        self.recalculating = true;
        let after: Vec<Pos> = match up_to_incl {
            Some(i) => self.path.blocks()[i + 1..].to_vec(),
            None => Vec::new(),
        };
        let complete = self.complete_path;
        let dst = match up_to_incl {
            Some(i) => fix_destination(self.path.get(i), env.player_y(), env.is_nether()),
            None => env.destination_fixed(),
        };
        self.job(env.player_feet(), dst, JobKind::RecalcSegment, Join::Append { after, complete, up_to_given: up_to_incl.is_some() }, note)
    }

    /// `pathNextSegment`: the stretch after node `after_incl`, put after the path up to there.
    pub fn path_next_segment(&mut self, after_incl: usize, env: &dyn PathEnv) -> Option<Job> {
        if self.recalculating {
            return None;
        }
        self.recalculating = true;
        let before = self.path.blocks()[..=after_incl].to_vec();
        let path_start = self.path.get(after_incl);
        let mut job = self.job(path_start, env.destination_fixed(), JobKind::NextSegment { path_start }, Join::Prepend { before: before.clone() }, None);
        job.before_len = before.len();
        Some(job)
    }

    /// `setPath`.
    fn set_path(&mut self, segment: &UnpackedSegment, env: &dyn PathEnv, out: &mut Finished) {
        let mut path = segment.collect();
        if self.append_destination {
            let dest = env.destination_fixed();
            let last = path.last().copied();
            match last {
                Some(last) if env.clear_view(V3::from_block(dest), V3::from_block(last), false) => path.push(dest),
                _ => {
                    self.direct(format!("unable to land at {}", block_pos_text(dest)));
                    out.bad_landing_spot = Some(dest);
                }
            }
        }
        self.path = NetherPath::new(path);
        self.complete_path = segment.is_finished();
        self.player_near = 0;
        self.ticks_near_unchanged = 0;
        self.max_player_near = 0;
    }

    /// What follows a search: put the answer in (or say it failed) and let the next one begin.
    pub fn finish(&mut self, job: Job, result: Result<UnpackedSegment, PathCalculationError>, env: &dyn PathEnv) -> Finished {
        let mut out = Finished::default();
        let secs = job.started.elapsed().as_secs_f64();
        match result {
            Ok(segment) => {
                let combined = match &job.join {
                    Join::Identity => segment,
                    Join::Append { after, complete, up_to_given } => {
                        let finished = *complete || (segment.is_finished() && !up_to_given);
                        segment.append(after, finished)
                    }
                    Join::Prepend { before } => segment.prepend(before),
                };
                self.set_path(&combined, env, &mut out);
                match &job.kind {
                    JobKind::Destination => {
                        if !self.path.is_empty() {
                            let distance = dist(self.path.get(0), self.path.get(self.path.len() - 1));
                            self.computed_note(distance, secs);
                        }
                    }
                    JobKind::NextSegment { .. } => {
                        let recompute = self.path.len() as i64 - job.before_len as i64 - 1;
                        let distance = if recompute > 0 { dist(self.path.get(0), self.path.get(recompute as usize)) } else { 0.0 };
                        self.computed_note(distance, secs);
                    }
                    JobKind::RecalcSegment => {}
                }
                if let Some(note) = &job.note_on_success {
                    self.verbose(note.replace("{secs}", &format!("{secs:.4}")));
                }
                self.recalculating = false;
            }
            Err(_) => {
                self.recalculating = false;
                match &job.kind {
                    JobKind::Destination => self.direct("Failed to compute path to destination".into()),
                    JobKind::RecalcSegment => self.direct("Failed to recompute segment".into()),
                    JobKind::NextSegment { path_start } => {
                        self.direct("Failed to compute next segment".into());
                        if env.player_distance_sqr_to_center(*path_start) < 16.0 * 16.0 {
                            self.verbose("Player is near the segment start, therefore repeating this calculation is pointless. Marking as complete".into());
                            self.complete_path = true;
                        }
                    }
                }
            }
        }
        out
    }

    fn computed_note(&mut self, distance: f64, secs: f64) {
        if self.complete_path {
            self.verbose(format!("Computed path ({distance:.1} blocks in {secs:.4} seconds)"));
        } else {
            self.verbose(format!("Computed segment (Next {distance:.1} blocks in {secs:.4} seconds)"));
        }
    }

    /// Required: the pathfinder's read lock is held.
    fn pathfind_around_obstacles(&mut self, env: &dyn PathEnv) -> Option<Job> {
        if self.recalculating {
            return None;
        }

        let range_start_incl = self.player_near;
        let mut range_end_excl = self.player_near;
        while range_end_excl < self.path.len() && {
            let p = self.path.get(range_end_excl);
            env.npf_has_chunk(p[0] >> 4, p[2] >> 4)
        } {
            range_end_excl += 1;
        }
        // range_end_excl now represents an index either not in the path, or just outside render distance.
        if range_start_incl >= range_end_excl {
            // Not loaded yet?
            return None;
        }
        let range_start = self.path.get(range_start_incl);
        if !env.passable(range_start[0], range_start[1], range_start[2], false) {
            // We're in a wall. Previous iterations of this function SHOULD have fixed this by now.
            return None;
        }

        if !env.is_landing() && self.ticks_near_unchanged > 100 {
            let job = self.path_recalc_segment(Some(range_end_excl - 1), env, Some("Recalculating segment, no progress in last 100 ticks".into()));
            self.ticks_near_unchanged = 0;
            return Some(job);
        }

        let mut can_see_any = false;
        for i in range_start_incl..range_end_excl.saturating_sub(1) {
            // Once we can see one node there's no point casting two more rays to every node after it, and those rays get
            // longer the further along the path we go. The range runs as far as the npf has chunks, which includes regions
            // it read off disk, so this could be thousands of rays a tick.
            if !can_see_any && (env.clear_view(env.player_feet_vec(), V3::from_block(self.path.get(i)), false) || env.clear_view(env.player_head(), V3::from_block(self.path.get(i)), false)) {
                can_see_any = true;
            }
            if !env.clear_view(V3::from_block(self.path.get(i)), V3::from_block(self.path.get(i + 1)), false) {
                // Obstacle. Where do we return to pathing? If the end of render distance is closer to goal, then that's
                // fine, otherwise we'd be "digging our hole deeper" and making an already bad backtrack worse.
                let dest = env.destination_fixed();
                let rejoin_main_path_at = if dist_sq(self.path.get(range_end_excl - 1), dest) < dist_sq(env.player_feet(), dest) {
                    Some(range_end_excl - 1) // rejoin after current render distance
                } else {
                    None // large backtrack detected. ignore render distance, rejoin later on
                };

                let blockage = self.path.get(i);
                let distance = dist(env.player_feet(), self.path.get(rejoin_main_path_at.unwrap_or(self.path.len() - 1)));
                let note = format!("Recalculated segment around path blockage near {} {} {} (next {distance:.1} blocks in {{secs}} seconds)", blockage[0], blockage[1], blockage[2]);
                return Some(self.path_recalc_segment(rejoin_main_path_at, env, Some(note)));
            }
        }
        if !can_see_any && range_start_incl + 2 < range_end_excl && !env.is_get_to_jump() {
            return Some(self.path_recalc_segment(Some(range_end_excl - 1), env, Some("Recalculated segment since no path points were visible".into())));
        }
        None
    }

    fn attempt_next_segment(&mut self, env: &dyn PathEnv) -> Option<Job> {
        if self.recalculating || self.path.is_empty() {
            return None;
        }

        let last = self.path.len() - 1;
        let last_pos = self.path.get(last);
        // `ctx.world().isLoaded` cannot be used here as it returns false is the y-value is beyond the build limits.
        if !self.complete_path && env.world_has_chunk(last_pos[0] >> 4, last_pos[2] >> 4) {
            return self.path_next_segment(last, env);
        }
        None
    }

    pub fn update_player_near(&mut self, pos: Pos) {
        if self.path.is_empty() {
            return;
        }
        let path = &self.path;
        let len = path.len() as i64;
        let d = |i: i64| dist_sq(path.get(i as usize), pos);

        let mut index = self.player_near as i64;
        // The bound of each loop follows the index as it moves.
        let mut i = index;
        while i >= (index - 1000).max(0) {
            if d(i) < d(index) {
                index = i; // intentional: this changes the bound of the loop
            }
            i -= 10;
        }
        let mut i = index;
        while i < (index + 1000).min(len) {
            if d(i) < d(index) {
                index = i;
            }
            i += 10;
        }
        let mut i = index;
        while i >= (index - 50).max(0) {
            if d(i) < d(index) {
                index = i;
            }
            i -= 1;
        }
        let mut i = index;
        while i < (index + 50).min(len) {
            if d(i) < d(index) {
                index = i;
            }
            i += 1;
        }
        self.player_near = index as usize;
    }
}

/// `ElytraBehavior.fixDestination`: in the nether, a player above the roof (y 128) goes to a destination at the roof, one
/// below it to one at y 64.
pub fn fix_destination(dst: Pos, player_y: f64, in_nether: bool) -> Pos {
    if in_nether {
        if player_y >= 128.0 && dst[1] < 128 {
            return [dst[0], 128, dst[2]];
        } else if player_y < 128.0 && dst[1] >= 128 {
            return [dst[0], 64, dst[2]];
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open air, except what is listed as solid. The player stands at `feet`.
    struct Env {
        feet: Pos,
        solid: Vec<Pos>,
        chunks_up_to_x: i32,
        landing: bool,
        flying: bool,
        dest: Pos,
    }

    impl FlightWorld for Env {
        fn passable(&self, x: i32, y: i32, z: i32, _: bool) -> bool {
            !self.solid.contains(&[x, y, z])
        }
        fn clear_view(&self, a: V3, b: V3, _: bool) -> bool {
            // A wall of solid blocks blocks any ray that crosses its x.
            !self.solid.iter().any(|s| (a.x.min(b.x)..=a.x.max(b.x)).contains(&f64::from(s[0])))
        }
        fn all_clear(&self, _: &[f64; 24], _: &[f64; 24]) -> bool {
            true
        }
    }

    impl PathEnv for Env {
        fn player_feet(&self) -> Pos {
            self.feet
        }
        fn player_y(&self) -> f64 {
            f64::from(self.feet[1])
        }
        fn is_nether(&self) -> bool {
            false
        }
        fn player_feet_vec(&self) -> V3 {
            V3::from_block(self.feet)
        }
        fn player_head(&self) -> V3 {
            V3::from_block(self.feet) + V3::new(0.0, 1.6, 0.0)
        }
        fn is_fall_flying(&self) -> bool {
            self.flying
        }
        fn npf_has_chunk(&self, chunk_x: i32, _: i32) -> bool {
            chunk_x <= self.chunks_up_to_x
        }
        fn world_has_chunk(&self, chunk_x: i32, _: i32) -> bool {
            chunk_x <= self.chunks_up_to_x
        }
        fn destination_fixed(&self) -> Pos {
            self.dest
        }
        fn is_landing(&self) -> bool {
            self.landing
        }
        fn is_get_to_jump(&self) -> bool {
            false
        }
        fn player_distance_sqr_to_center(&self, pos: Pos) -> f64 {
            dist_sq(self.feet, pos)
        }
    }

    fn env() -> Env {
        Env { feet: [0, 100, 0], solid: vec![], chunks_up_to_x: 100, landing: false, flying: true, dest: [200, 100, 0] }
    }

    fn straight(n: i32) -> Vec<Pos> {
        (0..n).map(|i| [i * 10, 100, 0]).collect()
    }

    fn start_with(m: &mut PathManager, e: &Env, blocks: Vec<Pos>, finished: bool) {
        let job = m.path_to_destination(e.feet, e);
        m.finish(job, Ok(UnpackedSegment::new(blocks, finished)), e);
    }

    #[test]
    fn the_player_is_matched_to_the_nearest_node_of_a_long_path() {
        let mut m = PathManager::new(false);
        let e = env();
        start_with(&mut m, &e, straight(30), true);
        m.update_player_near([123, 100, 2]);
        assert_eq!(m.near(), 12);
        // And back again.
        m.update_player_near([31, 100, 0]);
        assert_eq!(m.near(), 3);
    }

    #[test]
    fn a_finished_search_sets_the_path_and_says_how_far_it_goes() {
        let mut m = PathManager::new(false);
        let e = env();
        start_with(&mut m, &e, straight(5), true);
        assert_eq!(m.path().len(), 5);
        assert!(m.is_complete());
        let notes = m.take_notes();
        assert!(matches!(&notes[0], Note::Verbose(s) if s.starts_with("Computed path (40.0 blocks in")), "{notes:?}");
    }

    #[test]
    fn a_failed_search_says_so_and_clears_the_flag() {
        let mut m = PathManager::new(false);
        let e = env();
        let job = m.path_to_destination(e.feet, &e);
        m.finish(job, Err(PathCalculationError("Path calculation failed".into())), &e);
        assert_eq!(m.take_notes(), vec![Note::Direct("Failed to compute path to destination".into())]);
        assert!(!m.is_recalculating());
    }

    #[test]
    fn the_destination_is_appended_when_it_can_be_seen_and_reported_when_it_cannot() {
        let e = env();
        let mut m = PathManager::new(true);
        start_with(&mut m, &e, straight(5), true);
        assert_eq!(m.path().last(), Some([200, 100, 0]));

        let mut blocked = env();
        blocked.solid.push([100, 100, 0]);
        let mut m = PathManager::new(true);
        let job = m.path_to_destination(blocked.feet, &blocked);
        let out = m.finish(job, Ok(UnpackedSegment::new(straight(5), true)), &blocked);
        assert_eq!(out.bad_landing_spot, Some([200, 100, 0]));
        assert!(m.take_notes().contains(&Note::Direct("unable to land at BlockPos{x=200, y=100, z=0}".into())));
        assert_eq!(m.path().last(), Some([40, 100, 0]));
    }

    #[test]
    fn an_obstacle_between_two_nodes_starts_a_recalculation_that_keeps_the_rest_of_the_path() {
        let mut m = PathManager::new(false);
        let mut e = env();
        start_with(&mut m, &e, straight(20), true);
        e.chunks_up_to_x = 6; // chunks up to x 111 (node 11) are known
        e.solid.push([55, 100, 0]); // between nodes 5 and 6
        let job = m.tick(&e).expect("a recalculation");
        assert_eq!(*job.kind(), JobKind::RecalcSegment);
        assert!(m.is_recalculating());
        assert_eq!(job.src, e.feet);
        // The answer is joined to the nodes after the known stretch (nodes 7 on, as the end of the render distance is
        // closer to the goal than the player).
        let answer = UnpackedSegment::new(vec![[0, 100, 0], [30, 120, 0], [70, 100, 0]], false);
        let after_len = m.path().len();
        m.finish(job, Ok(answer), &e);
        assert!(!m.is_recalculating());
        assert!(m.path().len() < after_len + 3 && m.path().len() > 3);
        assert!(m.is_complete(), "the path was complete, and a recalculated part does not make it less so");
    }

    #[test]
    fn no_progress_for_a_hundred_ticks_recalculates_unless_landing() {
        let mut e = env();
        let mut m = PathManager::new(false);
        start_with(&mut m, &e, straight(20), true);
        let mut last = None;
        for _ in 0..102 {
            last = m.tick(&e);
            if last.is_some() {
                break;
            }
        }
        assert!(last.is_some(), "stuck for 100 ticks");
        let mut landing = PathManager::new(false);
        e.landing = true;
        start_with(&mut landing, &e, straight(20), true);
        assert!((0..150).all(|_| landing.tick(&e).is_none()));
    }

    #[test]
    fn an_unfinished_path_asks_for_the_next_segment_once_its_end_is_in_a_loaded_chunk() {
        let mut m = PathManager::new(false);
        let mut e = env();
        start_with(&mut m, &e, straight(5), false);
        e.chunks_up_to_x = 1; // the end of the path (x 40, chunk 2) is not loaded
        assert!(m.tick(&e).is_none());
        e.chunks_up_to_x = 5;
        let job = m.tick(&e).expect("the next segment");
        assert_eq!(*job.kind(), JobKind::NextSegment { path_start: [40, 100, 0] });
        assert_eq!(job.src, [40, 100, 0]);
        // Failing near the start of the segment gives up on it.
        e.feet = [38, 100, 0];
        m.finish(job, Err(PathCalculationError("x".into())), &e);
        assert!(m.is_complete());
    }

    #[test]
    fn the_destination_follows_the_roof_in_the_nether() {
        assert_eq!(fix_destination([5, 64, 5], 150.0, true), [5, 128, 5]);
        assert_eq!(fix_destination([5, 200, 5], 90.0, true), [5, 64, 5]);
        assert_eq!(fix_destination([5, 200, 5], 90.0, false), [5, 200, 5]);
        assert_eq!(fix_destination([5, 100, 5], 90.0, true), [5, 100, 5]);
    }
}
