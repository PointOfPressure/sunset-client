// baritone.process.elytra.pathfinder.Raytracer: a ray through the chunk octrees, "An efficient parametric algorithm for
// octree traversal" (Revelles, Urena, Lastra), as the native Refiner.cpp had it. The traversal only walks positive
// directions, so the ray is reflected around each x16 node's centre and the child indices are flipped back with `a`.
use super::baritone_elytra_chunk::Chunk;
use super::baritone_elytra_pathfinder::{CacheMiss, NetherPathfinder};

const PLANE_YZ: u8 = 0;
const PLANE_XZ: u8 = 1;
const PLANE_XY: u8 = 2;

/// What one x16 step reported.
enum Step {
    /// The ray left the node through this plane.
    Miss(u8),
    Finished,
    /// The length along the ray at which it hit.
    Hit(f64),
}

fn reflect(x: f64, target: f64) -> f64 {
    target - (x - target)
}

// Java's max/min keep x when y is NaN.
fn max(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        x
    } else if x > y {
        x
    } else {
        y
    }
}

fn min(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        x
    } else if x < y {
        x
    } else {
        y
    }
}

fn exit_plane(tx1: f64, ty1: f64, tz1: f64) -> u8 {
    if tx1 < ty1 {
        if tx1 < tz1 {
            return PLANE_YZ;
        }
    } else if ty1 < tz1 {
        return PLANE_XZ;
    }
    PLANE_XY
}

fn first_node(tx0: f64, ty0: f64, tz0: f64, txm: f64, tym: f64, tzm: f64) -> i32 {
    let mut answer = 0;
    if tx0 > ty0 {
        if tx0 > tz0 {
            // plane YZ
            if tym < tx0 {
                answer |= 2;
            }
            if tzm < tx0 {
                answer |= 1;
            }
            return answer;
        }
    } else if ty0 > tz0 {
        // plane XZ
        if txm < ty0 {
            answer |= 4;
        }
        if tzm < ty0 {
            answer |= 1;
        }
        return answer;
    }
    // plane XY
    if txm < tz0 {
        answer |= 4;
    }
    if tym < tz0 {
        answer |= 2;
    }
    answer
}

fn new_node(txm: f64, x: i32, tym: f64, y: i32, tzm: f64, z: i32) -> i32 {
    let mut out = z; // XY plane
    if txm < tym {
        if txm < tzm {
            out = x; // YZ plane
        }
    } else if tym < tzm {
        out = y; // XZ plane
    }
    out
}

fn m(origin: f64, node_min: i32, node_max: i32, t0: f64, t1: f64) -> f64 {
    if t0 == f64::INFINITY || t1 == f64::INFINITY {
        // 3.3
        let center = (node_min + node_max) / 2;
        return if origin < f64::from(center) { f64::INFINITY } else { f64::NEG_INFINITY };
    }
    0.5 * (t0 + t1)
}

/// Bytes of a child of a node at each level: x16 (4) has x8 children of 64 bytes, and so on. x2's children are bits.
const CHILD_BYTES: [usize; 5] = [0, 0, 1, 8, 64];

fn empty_node(section: &[u64; 64], filled: u32, off: usize, level: i32) -> bool {
    match level {
        4 => filled == 0,
        3 => filled & (1 << (off >> 6)) == 0, // the x8 at byte offset off is x8 number off / 64
        2 => section[off >> 3] == 0,
        _ => Chunk::byte_at(section, off) == 0,
    }
}

/// The parts of a ray that do not change while it walks one x16 node.
struct Ray {
    a: i32,
    ox: f64,
    oy: f64,
    oz: f64,
    target_len: f64,
}

/// Walks the ray through one node. level 4 is an x16 (the whole section), 0 a block; a node is the section plus a byte
/// offset, and at level 0 `bit` says which bit of the byte. `filled` is the section's summary of which of its x8 cubes
/// hold a block, see `Chunk::filled`. NaN is a miss, positive infinity means the ray ended inside without hitting, and
/// anything else is the length along the ray at which it hit.
#[allow(clippy::too_many_arguments)]
fn proc_subtree(
    r: &Ray,
    tx0: f64,
    ty0: f64,
    tz0: f64,
    tx1: f64,
    ty1: f64,
    tz1: f64,
    level: i32,
    nx: i32,
    ny: i32,
    nz: i32,
    section: Option<&[u64; 64]>,
    filled: u32,
    off: usize,
    bit: i32,
) -> f64 {
    // If this node is behind us.
    if tx1 < 0.0 || ty1 < 0.0 || tz1 < 0.0 {
        return f64::NAN;
    }
    if tx0 >= r.target_len || ty0 >= r.target_len || tz0 >= r.target_len {
        return f64::INFINITY;
    }
    if level == 0 {
        // A leaf.
        let Some(section) = section else { return f64::NAN };
        if (i32::from(Chunk::byte_at(section, off)) >> bit) & 1 == 0 {
            return f64::NAN;
        }
        return if tx0 > ty0 && tx0 > tz0 {
            tx0 // plane YZ
        } else if ty0 > tz0 {
            ty0 // plane XZ
        } else {
            tz0 // plane XY
        };
    }
    let Some(section) = section else { return f64::NAN };
    if empty_node(section, filled, off, level) {
        // We know that all the leafs are empty.
        return f64::NAN;
    }
    let w = 1 << level;
    let txm = m(r.ox, nx, nx + w, tx0, tx1);
    let tym = m(r.oy, ny, ny + w, ty0, ty1);
    let tzm = m(r.oz, nz, nz + w, tz0, tz1);

    let half = w >> 1;
    let child_level = level - 1;
    let child_bytes = CHILD_BYTES[level as usize];
    let mut curr_node = first_node(tx0, ty0, tz0, txm, tym, tzm);
    loop {
        // Child curr_node of this node in the reflected frame; xor with a gives the real octant, i. Its origin is the
        // node's, moved by half the width along each axis whose bit is set in i (4 is x, 2 is y, 1 is z), and its bytes
        // start i children in from the node's -- except below an x2, whose children are the bits of one byte, which i
        // indexes instead.
        let i = curr_node ^ r.a;
        let cx = nx + if i & 4 != 0 { half } else { 0 };
        let cy = ny + if i & 2 != 0 { half } else { 0 };
        let cz = nz + if i & 1 != 0 { half } else { 0 };
        let child_off = if level == 1 { off } else { off + i as usize * child_bytes };
        let descend = |a0: f64, a1: f64, a2: f64, b0: f64, b1: f64, b2: f64| {
            proc_subtree(r, a0, a1, a2, b0, b1, b2, child_level, cx, cy, cz, Some(section), filled, child_off, i)
        };
        let res;
        match curr_node {
            0 => {
                res = descend(tx0, ty0, tz0, txm, tym, tzm);
                if !res.is_nan() {
                    return res;
                }
                curr_node = new_node(txm, 4, tym, 2, tzm, 1);
            }
            1 => {
                res = descend(tx0, ty0, tzm, txm, tym, tz1);
                if !res.is_nan() {
                    return res;
                }
                curr_node = new_node(txm, 5, tym, 3, tz1, 8);
            }
            2 => {
                res = descend(tx0, tym, tz0, txm, ty1, tzm);
                if !res.is_nan() {
                    return res;
                }
                curr_node = new_node(txm, 6, ty1, 8, tzm, 3);
            }
            3 => {
                res = descend(tx0, tym, tzm, txm, ty1, tz1);
                if !res.is_nan() {
                    return res;
                }
                curr_node = new_node(txm, 7, ty1, 8, tz1, 8);
            }
            4 => {
                res = descend(txm, ty0, tz0, tx1, tym, tzm);
                if !res.is_nan() {
                    return res;
                }
                curr_node = new_node(tx1, 8, tym, 6, tzm, 5);
            }
            5 => {
                res = descend(txm, ty0, tzm, tx1, tym, tz1);
                if !res.is_nan() {
                    return res;
                }
                curr_node = new_node(tx1, 8, tym, 7, tz1, 8);
            }
            6 => {
                res = descend(txm, tym, tz0, tx1, ty1, tzm);
                if !res.is_nan() {
                    return res;
                }
                curr_node = new_node(tx1, 8, ty1, 8, tzm, 7);
            }
            _ => {
                res = descend(txm, tym, tzm, tx1, ty1, tz1);
                return if res.is_nan() { f64::NAN } else { res };
            }
        }
        if curr_node >= 8 {
            return f64::NAN;
        }
    }
}

/// One x16 node. The ray is already reflected.
#[allow(clippy::too_many_arguments)]
fn raytrace16(a: i32, rox: f64, roy: f64, roz: f64, rdx: f64, rdy: f64, rdz: f64, target_len: f64, nx: i32, ny: i32, nz: i32, section: Option<&[u64; 64]>, filled: u32) -> Step {
    // IEEE stability fix.
    let divx = 1.0 / rdx;
    let divy = 1.0 / rdy;
    let divz = 1.0 / rdz;

    let tx0 = (f64::from(nx) - rox) * divx;
    let tx1 = (f64::from(nx + 16) - rox) * divx;
    let ty0 = (f64::from(ny) - roy) * divy;
    let ty1 = (f64::from(ny + 16) - roy) * divy;
    let tz0 = (f64::from(nz) - roz) * divz;
    let tz1 = (f64::from(nz + 16) - roz) * divz;

    // Condition 10.
    let tmin = max(max(tx0, ty0), tz0);
    let tmax = min(min(tx1, ty1), tz1);
    if tmin <= tmax {
        let ray = Ray { a, ox: rox, oy: roy, oz: roz, target_len };
        let r = proc_subtree(&ray, tx0, ty0, tz0, tx1, ty1, tz1, 4, nx, ny, nz, section, filled, 0, 0);
        if r == f64::INFINITY {
            return Step::Finished;
        }
        if !r.is_nan() {
            return Step::Hit(r);
        }
        return Step::Miss(exit_plane(tx1, ty1, tz1));
    }
    // The ray only touches this node along an edge or at a corner, and rounding made the interval empty. Either the ray
    // ends there, or it continues into the node on the other side of that edge; either way there is nothing to test in
    // this node.
    if tmin >= target_len {
        return Step::Finished;
    }
    Step::Miss(exit_plane(tx1, ty1, tz1))
}

/// Traces the segment from `from` to `to`. Returns where it hit a solid block, or None if it reached its end. The two
/// points must be inside 0 <= y < 384, as must be the segment between them. A segment of no length is a point, which is
/// a hit if it is inside a block; a coordinate that is not a finite number is refused.
pub fn raytrace(ctx: &NetherPathfinder, from: [f64; 3], to: [f64; 3], fake_chunk_mode: CacheMiss) -> Result<Option<[f64; 3]>, String> {
    let [fx, fy, fz] = from;
    let [tx, ty, tz] = to;
    let (vx, vy, vz) = (tx - fx, ty - fy, tz - fz);
    let target_len = (vx * vx + vy * vy + vz * vz).sqrt();
    if !(target_len < f64::INFINITY) {
        // NaN, or an infinite coordinate: dividing by it would make the direction NaN and the walk below would step from
        // node to node for ever. The native library did just that.
        return Err(format!("ray with a coordinate that is not a finite number: ({fx}, {fy}, {fz}) to ({tx}, {ty}, {tz})"));
    }
    if target_len == 0.0 {
        // A point. A ray that starts inside a block reports a hit at its origin, so a point does the same, and one in the
        // air hits nothing.
        let (bx, by, bz) = (fx.floor() as i32, fy.floor() as i32, fz.floor() as i32);
        let chunk = ctx.real_chunk_from_cache_or_fake_chunk_maybe_gen(bx >> 4, bz >> 4, fake_chunk_mode);
        if !(0..super::baritone_elytra_chunk::HEIGHT).contains(&by) || !chunk.is_solid(bx & 15, by, bz & 15) {
            return Ok(None);
        }
        return Ok(Some([fx, fy, fz]));
    }
    let (dx, dy, dz) = (vx / target_len, vy / target_len, vz / target_len);
    let mut a = 0;
    if dx < 0.0 {
        a |= 4;
    }
    if dy < 0.0 {
        a |= 2;
    }
    if dz < 0.0 {
        a |= 1;
    }

    let mut nx = fx.floor() as i32 & !15;
    let mut ny = fy.floor() as i32 & !15;
    let mut nz = fz.floor() as i32 & !15;
    loop {
        let chunk = ctx.real_chunk_from_cache_or_fake_chunk_maybe_gen(nx >> 4, nz >> 4, fake_chunk_mode);
        let section = chunk.section(ny);
        // Reflect around the node's centre so the traversal only has to walk positive directions.
        let (mut rox, mut roy, mut roz, mut rdx, mut rdy, mut rdz) = (fx, fy, fz, dx, dy, dz);
        if a & 4 != 0 {
            rox = reflect(fx, f64::from(nx + 8));
            rdx = -dx;
        }
        if a & 2 != 0 {
            roy = reflect(fy, f64::from(ny + 8));
            rdy = -dy;
        }
        if a & 1 != 0 {
            roz = reflect(fz, f64::from(nz + 8));
            rdz = -dz;
        }
        match raytrace16(a, rox, roy, roz, rdx, rdy, rdz, target_len, nx, ny, nz, section, u32::from(chunk.filled(ny))) {
            Step::Finished => return Ok(None),
            Step::Hit(len) => {
                if len < 0.0 {
                    // The origin was inside an occupied block, so the hit is the origin.
                    return Ok(Some([fx, fy, fz]));
                }
                return Ok(Some([fx + dx * len, fy + dy * len, fz + dz * len]));
            }
            Step::Miss(PLANE_XY) => nz += if a & 1 != 0 { -16 } else { 16 },
            Step::Miss(PLANE_XZ) => ny += if a & 2 != 0 { -16 } else { 16 },
            Step::Miss(_) => nx += if a & 4 != 0 { -16 } else { 16 },
        }
    }
}

fn last_visible_node(ctx: &NetherPathfinder, path: &[[i32; 3]], current_node: usize) -> usize {
    if current_node == path.len() - 1 {
        return current_node;
    }
    let from_block = path[current_node];
    let mut last_visible = current_node + 1; // can assume that the next node is always visible from the previous
    for i in last_visible + 1..path.len() {
        let current_block = path[i];
        if from_block == current_block {
            continue; // the pathfinder can produce 2 consecutive equal points and that breaks the raytracer
        }
        let from = from_block.map(f64::from);
        let to = current_block.map(f64::from);
        // Coordinates that are whole numbers are always finite, so the error cannot happen.
        if raytrace(ctx, from, to, CacheMiss::Generate).ok().flatten().is_some() {
            return last_visible;
        }
        last_visible = i;
    }
    last_visible
}

/// Drops the points of a path that are visible from an earlier one.
pub fn refine(ctx: &NetherPathfinder, path: &[[i32; 3]]) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < path.len() {
        out.push(path[i]);
        if i == path.len() - 1 {
            break;
        }
        i = last_visible_node(ctx, path, i);
    }
    out
}
