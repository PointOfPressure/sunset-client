// The elytra pathfinder against what the native library answered (tests/data, recorded by the Java project's oracle) and
// the Java project's own tests of the search, the rays and the cancel flag.
use super::baritone_elytra_chunk::Chunk;
use super::baritone_elytra_generator::ChunkGeneratorHell;
use super::baritone_elytra_pathfinder::{CacheMiss, Dimension, NetherPathfinder, PathSegment};
use super::baritone_elytra_raytracer::raytrace;
use std::sync::Arc;
use std::time::Instant;

const SEED: i64 = 146_008_555_100_680;
const SOLID: CacheMiss = CacheMiss::Solid;

fn context(seed: i64) -> NetherPathfinder {
    NetherPathfinder::new(seed, None, Dimension::Nether, 128).unwrap()
}

/// A context over the generated chunks x, z in 0..side-1, inserted as if the game had given them.
fn generated_world(side: i32) -> NetherPathfinder {
    let ctx = context(SEED);
    let generator = ChunkGeneratorHell::from_seed(SEED);
    for x in 0..side {
        for z in 0..side {
            ctx.insert_chunk(x, z, generator.generate_chunk(x, z));
        }
    }
    ctx
}

fn visible(ctx: &NetherPathfinder, mode: CacheMiss, from: [f64; 3], to: [f64; 3]) -> bool {
    raytrace(ctx, from, to, mode).unwrap().is_none()
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[test]
fn every_chunk_matches_the_native_generator_bit_for_bit() {
    let expected = include_str!("../tests/data/chunk-hashes.txt");
    let generator = ChunkGeneratorHell::from_seed(SEED);
    let mut count = 0;
    let mut wrong = Vec::new();
    for line in expected.lines().filter(|l| !l.is_empty()) {
        let mut parts = line.split(' ');
        let x: i32 = parts.next().unwrap().parse().unwrap();
        let z: i32 = parts.next().unwrap().parse().unwrap();
        let hash = u64::from_str_radix(parts.next().unwrap(), 16).unwrap();
        if fnv1a(&generator.generate_chunk(x, z).to_bytes()) != hash {
            wrong.push((x, z));
        }
        count += 1;
    }
    assert_eq!(count, 25 * 25);
    assert!(wrong.is_empty(), "chunks that differ from the native generator: {wrong:?}");
}

#[test]
fn generation_is_deterministic() {
    let a = ChunkGeneratorHell::from_seed(SEED);
    let b = ChunkGeneratorHell::from_seed(SEED);
    assert_eq!(fnv1a(&a.generate_chunk(100, -7).to_bytes()), fnv1a(&b.generate_chunk(100, -7).to_bytes()));
}

#[test]
fn every_oracle_ray_agrees_with_the_native_raytracer() {
    let bytes = include_bytes!("../tests/data/rays.bin");
    let count = i32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    assert_eq!(count, 4000);
    let ctx = generated_world(16);
    let double = |at: usize| f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let mut at = 4;
    let (mut wrong_hit, mut wrong_where) = (0, 0);
    for _ in 0..count {
        let from = [double(at), double(at + 8), double(at + 16)];
        let to = [double(at + 24), double(at + 32), double(at + 40)];
        let hit = bytes[at + 48] != 0;
        let wh = [double(at + 49), double(at + 57), double(at + 65)];
        at += 73;
        match raytrace(&ctx, from, to, SOLID).unwrap() {
            Some(_) if !hit => wrong_hit += 1,
            None if hit => wrong_hit += 1,
            Some(found) if found != wh => wrong_where += 1,
            _ => {}
        }
    }
    assert_eq!(wrong_hit, 0, "rays that hit where the native raytracer missed, or the other way round");
    assert_eq!(wrong_where, 0, "hits at a different position than the native raytracer's");
}

#[test]
fn nothing_to_hit_in_air_mode() {
    assert!(visible(&context(1), CacheMiss::Air, [0.5, 64.5, 0.5], [300.5, 20.5, -200.5]));
}

#[test]
fn unknown_chunks_are_walls_in_solid_mode() {
    let hit = raytrace(&context(1), [0.5, 64.5, 0.5], [30.5, 64.5, 0.5], SOLID).unwrap();
    // The origin is inside the solid, so the hit is the origin.
    assert_eq!(hit, Some([0.5, 64.5, 0.5]));
}

#[test]
fn a_ray_hits_the_face_of_a_block() {
    let ctx = context(1);
    let mut chunk = Chunk::new();
    chunk.set_block(8, 64, 8, true);
    ctx.insert_chunk(0, 0, chunk);
    let hit = raytrace(&ctx, [2.5, 64.5, 8.5], [14.5, 64.5, 8.5], CacheMiss::Air).unwrap().unwrap();
    assert!((hit[0] - 8.0).abs() < 1e-9 && (hit[1] - 64.5).abs() < 1e-9 && (hit[2] - 8.5).abs() < 1e-9, "{hit:?}");
    // And from the other side.
    let hit = raytrace(&ctx, [14.5, 64.5, 8.5], [2.5, 64.5, 8.5], CacheMiss::Air).unwrap().unwrap();
    assert!((hit[0] - 9.0).abs() < 1e-9);
    // A ray that stops short of it, and one past it in another row.
    assert!(visible(&ctx, CacheMiss::Air, [2.5, 64.5, 8.5], [7.5, 64.5, 8.5]));
    assert!(visible(&ctx, CacheMiss::Air, [2.5, 64.5, 9.5], [14.5, 64.5, 9.5]));
}

#[test]
fn rays_cross_chunk_and_slab_boundaries() {
    let ctx = context(1);
    ctx.insert_chunk(0, 0, Chunk::new());
    ctx.insert_chunk(1, 0, Chunk::new());
    let mut far = Chunk::new();
    far.set_block(3, 70, 4, true);
    ctx.insert_chunk(2, 0, far);
    assert!(visible(&ctx, CacheMiss::Air, [1.5, 40.5, 4.5], [34.5, 70.5, 4.5]));
    assert!(!visible(&ctx, CacheMiss::Air, [1.5, 40.5, 4.5], [36.5, 70.5, 4.5]));
    assert!(!visible(&ctx, CacheMiss::Air, [36.5, 70.5, 4.5], [1.5, 40.5, 4.5]));
}

#[test]
fn the_ray_that_ended_the_native_process_terminates() {
    // nether-pathfinder issue 23: an end exactly on a chunk corner.
    let (from, to) = ([386.7112215521066, 137.40911926818373, 7.0554159455922285], [416.0, 142.0, 0.0]);
    assert!(visible(&context(1), CacheMiss::Air, from, to));
    assert!(!visible(&context(1), CacheMiss::Solid, from, to));
}

/// Chunk (0, 0) from the game: section 3 (y 48 to 63) all solid, and one block at (5, 70, 5).
fn degenerate_world() -> NetherPathfinder {
    let ctx = context(1);
    let mut chunk = Chunk::new();
    chunk.fill_section(3, true);
    chunk.set_block(5, 70, 5, true);
    ctx.insert_chunk(0, 0, chunk);
    ctx
}

#[test]
fn a_point_is_visible_in_the_air_and_a_hit_inside_a_block() {
    let ctx = degenerate_world();
    assert!(visible(&ctx, SOLID, [3.5, 71.5, 3.5], [3.5, 71.5, 3.5]));
    assert!(!visible(&ctx, SOLID, [5.5, 70.5, 5.5], [5.5, 70.5, 5.5]));
    assert_eq!(raytrace(&ctx, [5.25, 70.5, 5.75], [5.25, 70.5, 5.75], SOLID).unwrap(), Some([5.25, 70.5, 5.75]));
    // A point outside the world is in the air, as a ray there is.
    assert!(visible(&ctx, SOLID, [5.5, 400.0, 5.5], [5.5, 400.0, 5.5]));
}

#[test]
fn a_coordinate_that_is_not_finite_is_refused() {
    let ctx = degenerate_world();
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for axis in 0..6 {
            let mut p = [3.5, 71.5, 3.5, 8.5, 71.5, 8.5];
            p[axis] = v;
            assert!(raytrace(&ctx, [p[0], p[1], p[2]], [p[3], p[4], p[5]], SOLID).is_err(), "accepted {v} at {axis}");
        }
    }
    // Both ends infinite: the length is NaN rather than infinite.
    assert!(raytrace(&ctx, [f64::INFINITY, 71.5, 3.5], [f64::INFINITY, 71.5, 8.5], SOLID).is_err());
}

#[test]
fn a_ray_ending_on_a_voxel_corner_in_the_air_answers_like_one_ending_just_inside() {
    // Every node of a flight path is a corner inside its air cube, like (8, 68, 8) here, four blocks above the solid
    // section and clear of the block at (5, 70, 5).
    let ctx = degenerate_world();
    let mut checked = 0;
    let mut ox = 0.3;
    while ox < 16.0 {
        let mut oz = 0.7;
        while oz < 16.0 {
            let mut oy = 40.2;
            while oy < 100.0 {
                let exact = visible(&ctx, SOLID, [ox, oy, oz], [8.0, 68.0, 8.0]);
                let nudge = 1e-6;
                let to = [8.0 + if ox < 8.0 { -nudge } else { nudge }, 68.0 + if oy < 68.0 { -nudge } else { nudge }, 8.0 + if oz < 8.0 { -nudge } else { nudge }];
                assert_eq!(visible(&ctx, SOLID, [ox, oy, oz], to), exact, "from {ox}, {oy}, {oz}");
                checked += 1;
                oy += 4.9;
            }
            oz += 1.9;
        }
        ox += 1.7;
    }
    assert!(checked > 500);
}

#[test]
fn a_ray_ending_on_a_corner_of_a_solid_block_answers() {
    // (8, 64, 8) is on the solid section's top face. Whether a ray that ends there grazes the section is a tie that
    // rounding settles either way; what matters is that it answers.
    let ctx = degenerate_world();
    let (mut blocked, mut clear) = (0, 0);
    let mut ox = 0.3;
    while ox < 16.0 {
        let mut oz = 0.7;
        while oz < 16.0 {
            let mut oy = 64.2;
            while oy < 100.0 {
                if visible(&ctx, SOLID, [ox, oy, oz], [8.0, 64.0, 8.0]) { clear += 1 } else { blocked += 1 }
                oy += 4.9;
            }
            oz += 1.9;
        }
        ox += 1.7;
    }
    assert!(clear > 0 && clear + blocked > 300, "{clear} clear, {blocked} blocked");
}

// ---- the search ----

fn find(ctx: &NetherPathfinder, from: [i32; 3], to: [i32; 3], refine: bool, timeout: u64, air: bool, fake_cost: f64) -> Option<PathSegment> {
    ctx.path_find(from, to, true, refine, timeout, air, fake_cost).unwrap()
}

#[test]
fn straight_through_air_when_nothing_is_known() {
    let segment = find(&context(1), [0, 60, 0], [500, 60, 0], false, 10_000, true, 8.0).expect("a path");
    assert!(segment.finished);
    assert!(segment.blocks.len() >= 2);
    let mut last_x = i32::MIN;
    for b in &segment.blocks {
        assert!(b[0] > last_x, "x goes forward");
        last_x = b[0];
        assert!(b[2].abs() <= 16, "z {}", b[2]);
        assert!((b[1] - 60).abs() <= 16, "y {}", b[1]);
    }
    assert!((last_x - 500).abs() <= 16, "ends at {last_x}");
}

#[test]
fn refining_a_straight_path_leaves_its_ends() {
    // Refining raytraces in generate mode, so the chunks have to be known and empty for the rays to pass.
    let ctx = context(1);
    for x in -1..=32 {
        for z in -1..=1 {
            ctx.insert_chunk(x, z, Chunk::new());
        }
    }
    let segment = find(&ctx, [0, 60, 0], [500, 60, 0], true, 10_000, true, 8.0).expect("a path");
    assert!(segment.finished);
    assert!(segment.blocks.len() <= 3, "refined to {} points", segment.blocks.len());
}

#[test]
fn it_gives_up_after_a_hundred_unknown_chunks() {
    let segment = find(&context(1), [0, 60, 0], [20_000, 60, 0], false, 10_000, true, 8.0).expect("a path");
    assert!(!segment.finished);
    assert!(segment.blocks.last().unwrap()[0] > 100);
}

#[test]
fn it_reaches_the_goal_over_real_terrain() {
    let ctx = generated_world(24);
    let segment = find(&ctx, [20, 60, 20], [340, 60, 200], false, 10_000, true, 8.0).expect("a path");
    assert!(segment.finished);
    // Every point of the path is in air.
    for b in &segment.blocks {
        assert!(!ctx.get_chunk_or_default(b[0] >> 4, b[2] >> 4, true).is_solid(b[0] & 15, b[1], b[2] & 15), "path goes through a block at {b:?}");
    }
}

#[test]
fn it_generates_terrain_when_asked_to() {
    let ctx = context(SEED);
    let segment = find(&ctx, [20, 60, 20], [300, 60, 20], false, 10_000, false, 1.0).expect("a path");
    assert!(segment.finished);
    assert!(ctx.chunk_count() > 20, "generated chunks along the way");
    assert!(!ctx.has_chunk_from_caller(1, 1));
}

#[test]
fn bad_arguments_are_refused() {
    assert!(context(1).path_find([0, -1, 0], [10, 60, 0], true, false, 0, true, 1.0).is_err());
    assert!(NetherPathfinder::new(1, None, Dimension::Nether, 500).is_err());
}

// ---- when a cancel takes effect ----

/// Real terrain, far enough away that it cannot arrive. Left alone it returns an unfinished segment once the primary
/// timeout is up, so "returned None quickly" means cancelled and nothing else.
fn long_search(ctx: &NetherPathfinder) -> Option<PathSegment> {
    find(ctx, [0, 50, 0], [100_000, 50, 0], false, 0, false, 1.0)
}

/// A few hundred blocks through known-empty chunks, which finishes in well under a second.
fn short_search(ctx: &NetherPathfinder) -> Option<PathSegment> {
    find(ctx, [0, 60, 0], [500, 60, 0], false, 10_000, true, 8.0)
}

#[test]
fn a_cancel_that_arrives_before_the_search_starts_still_stops_it() {
    let ctx = context(SEED);
    assert!(!ctx.cancel(), "nothing has asked it to stop yet");
    let started = Instant::now();
    let segment = long_search(&ctx);
    assert!(segment.is_none(), "a cancelled search has no path");
    assert!(started.elapsed().as_millis() < 400, "gave up at once rather than running the search out");
}

#[test]
fn a_cancel_applies_to_one_search_and_not_the_next() {
    let ctx = context(SEED);
    ctx.cancel();
    assert!(long_search(&ctx).is_none(), "the first search after the cancel is the one it stops");
    let segment = short_search(&ctx).expect("the search after a cancelled one runs normally");
    assert!(segment.finished);
}

#[test]
fn a_search_that_finishes_leaves_the_flag_clear() {
    let ctx = context(SEED);
    assert!(short_search(&ctx).is_some());
    assert!(!ctx.cancel(), "a completed search did not leave the flag set");
}

#[test]
fn cancel_stops_a_running_search_on_another_thread() {
    let ctx = Arc::new(context(SEED));
    let worker = ctx.clone();
    let started = Instant::now();
    let searcher = std::thread::spawn(move || long_search(&worker).is_none());
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(!ctx.cancel(), "not cancelled before");
    assert!(searcher.join().unwrap(), "the cancelled search has no path");
    assert!(started.elapsed().as_millis() < 1000, "returned after {} ms", started.elapsed().as_millis());
    assert!(!ctx.cancel(), "the cancelled search cleared the flag on its way out");
}

#[test]
fn a_search_that_cannot_finish_returns_what_it_has_after_half_a_second() {
    let ctx = context(SEED);
    let started = Instant::now();
    let segment = long_search(&ctx).expect("a path");
    let ms = started.elapsed().as_millis();
    assert!(!segment.finished);
    assert!((500..5000).contains(&ms), "took {ms} ms");
    assert!(segment.blocks.last().unwrap()[0] > 50);
}
