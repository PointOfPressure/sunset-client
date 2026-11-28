// com.seedfinding.mcseed.rand.Rand and JRand (java.util.Random over the Java LCG), and com.seedfinding.mccore.rand.ChunkRand
// (the seedings the world generator makes). A JRand always runs the Java LCG here: the Java's other LCGs are only used
// by CombinedJRand and the debugger, which nothing needs. Every method is the Java's, name for name in snake case.
use super::sunset_scx_lcg::LCG;
use super::sunset_scx_mth as mth;
use super::sunset_scx_version::MCVersion;

const DOUBLE_UNIT: f64 = 1.0 / 9_007_199_254_740_992.0; // 0x1.0p-53

pub const REGION_SEED_A: i64 = 341873128712;
pub const REGION_SEED_B: i64 = 132897987541;

#[derive(Clone, Debug)]
pub struct JRand {
    seed: i64,
    next_next_gaussian: f64,
    have_next_next_gaussian: bool,
}

impl JRand {
    /// `new JRand(seed)`: the seed is scrambled.
    pub fn new(seed: i64) -> JRand {
        Self::with_scramble(seed, true)
    }

    /// `new JRand(seed, scramble)`.
    pub fn with_scramble(seed: i64, scramble: bool) -> JRand {
        let mut r = JRand { seed: 0, next_next_gaussian: 0.0, have_next_next_gaussian: false };
        r.set_seed_scramble(seed, scramble);
        r
    }

    pub fn of_internal_seed(seed: i64) -> JRand {
        Self::with_scramble(seed, false)
    }

    pub fn of_scrambled_seed(seed: i64) -> JRand {
        Self::with_scramble(seed, true)
    }

    pub fn get_seed(&self) -> i64 {
        self.seed
    }

    /// `Rand.setSeed`: the seed as it is, no scrambling.
    pub fn set_raw_seed(&mut self, seed: i64) {
        self.seed = seed;
    }

    /// `JRand.setSeed(seed)`.
    pub fn set_seed(&mut self, seed: i64) {
        self.set_seed_scramble(seed, true);
    }

    pub fn set_seed_scramble(&mut self, seed: i64, scramble: bool) {
        self.seed = if scramble { seed ^ LCG::JAVA.multiplier } else { seed };
    }

    pub fn next_seed(&mut self) -> i64 {
        self.seed = LCG::JAVA.next_seed(self.seed);
        self.seed
    }

    pub fn advance(&mut self, calls: i64) {
        self.advance_lcg(&LCG::JAVA.combine_steps(calls));
    }

    pub fn advance_lcg(&mut self, skip: &LCG) {
        self.seed = skip.next_seed(self.seed);
    }

    pub fn next_bits(&mut self, bits: i32) -> i64 {
        self.seed = self.next_seed();
        ((self.seed as u64) >> (48 - bits)) as i64
    }

    pub fn next(&mut self, bits: i32) -> i32 {
        self.next_bits(bits) as i32
    }

    pub fn next_boolean(&mut self) -> bool {
        self.next(1) == 1
    }

    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if (bound & bound.wrapping_neg()) == bound {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    pub fn next_long(&mut self) -> i64 {
        ((self.next(32) as i64) << 32).wrapping_add(self.next(32) as i64)
    }

    pub fn next_double(&mut self) -> f64 {
        let hi = (self.next(26) as i64) << 27;
        (hi + self.next(27) as i64) as f64 * DOUBLE_UNIT
    }

    pub fn next_gaussian(&mut self) -> f64 {
        if self.have_next_next_gaussian {
            self.have_next_next_gaussian = false;
            return self.next_next_gaussian;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let multiplier = (-2.0 * s.ln() / s).sqrt();
                self.next_next_gaussian = v2 * multiplier;
                self.have_next_next_gaussian = true;
                return v1 * multiplier;
            }
        }
    }

    pub fn shuffle<T>(&mut self, list: &mut [T]) {
        let size = list.len();
        let mut i = size;
        while i > 1 {
            let j = self.next_int_bound(i as i32) as usize;
            list.swap(i - 1, j);
            i -= 1;
        }
    }

    pub fn copy(&self) -> JRand {
        JRand::with_scramble(self.seed, false)
    }

    // The static forms: what a call would answer from a given internal seed, with no generator at all.

    pub fn next_boolean_at(seed: i64) -> bool {
        ((seed as u64 >> 47) & 1) == 1
    }

    pub fn next_int_at(seed: i64) -> i32 {
        (seed as u64 >> 16) as i32
    }

    pub fn next_int_bound_at(mut seed: i64, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if (bound & bound.wrapping_neg()) == bound {
            return ((bound as i64).wrapping_mul(seed) >> 31) as i32;
        }
        loop {
            let bits = (seed as u64 >> 17) as i32;
            let value = bits % bound;
            seed = LCG::JAVA.next_seed(seed);
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    pub fn next_float_at(seed: i64) -> f32 {
        (seed as u64 >> 24) as i32 as f32 / (1 << 24) as f32
    }

    pub fn next_long_at(seed: i64) -> i64 {
        (((seed as u64) >> 16 << 32) as i64).wrapping_add((LCG::JAVA.next_seed(seed) as u64 >> 16) as i32 as i64)
    }

    pub fn next_double_at(seed: i64) -> f64 {
        let hi = ((seed as u64 >> 22) as i32 as i64) << 27;
        (hi + (LCG::JAVA.next_seed(seed) as u64 >> 21) as i32 as i64) as f64 * DOUBLE_UNIT
    }
}

/// `ChunkRand`: a JRand with the seedings of the world generator. They answer the 48 bits of the seed they set.
#[derive(Clone, Debug)]
pub struct ChunkRand {
    pub rand: JRand,
}

impl Default for ChunkRand {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for ChunkRand {
    type Target = JRand;
    fn deref(&self) -> &JRand {
        &self.rand
    }
}

impl std::ops::DerefMut for ChunkRand {
    fn deref_mut(&mut self) -> &mut JRand {
        &mut self.rand
    }
}

impl ChunkRand {
    /// `new ChunkRand()`: internal seed 0.
    pub fn new() -> ChunkRand {
        ChunkRand { rand: JRand::with_scramble(0, false) }
    }

    pub fn with_seed(seed: i64) -> ChunkRand {
        ChunkRand { rand: JRand::new(seed) }
    }

    pub fn with_seed_scramble(seed: i64, scramble: bool) -> ChunkRand {
        ChunkRand { rand: JRand::with_scramble(seed, scramble) }
    }

    pub fn set_terrain_seed(&mut self, chunk_x: i32, chunk_z: i32, _version: MCVersion) -> i64 {
        let seed = (chunk_x as i64).wrapping_mul(341873128712).wrapping_add((chunk_z as i64).wrapping_mul(132897987541));
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    pub fn set_population_seed(&mut self, world_seed: i64, x: i32, z: i32, version: MCVersion) -> i64 {
        self.set_seed(world_seed);
        let (a, b);
        if version.is_older_than(MCVersion::v1_13) {
            a = self.next_long() / 2 * 2 + 1;
            b = self.next_long() / 2 * 2 + 1;
        } else {
            a = self.next_long() | 1;
            b = self.next_long() | 1;
        }
        let seed = (x as i64).wrapping_mul(a).wrapping_add((z as i64).wrapping_mul(b)) ^ world_seed;
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    /// `setDecoratorSeed(populationSeed, index, step, version)`.
    pub fn set_decorator_seed_step(&mut self, population_seed: i64, index: i32, step: i32, version: MCVersion) -> i64 {
        self.set_decorator_seed_salt(population_seed, index + 10000 * step, version)
    }

    /// `setDecoratorSeed(populationSeed, salt, version)`.
    pub fn set_decorator_seed_salt(&mut self, population_seed: i64, salt: i32, version: MCVersion) -> i64 {
        assert!(!version.is_older_than(MCVersion::v1_13), "UnsupportedVersion: decorator seed");
        let seed = population_seed.wrapping_add(salt as i64);
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    /// `setDecoratorSeed(worldSeed, blockX, blockZ, index, step, version)`.
    pub fn set_decorator_seed_at_step(&mut self, world_seed: i64, block_x: i32, block_z: i32, index: i32, step: i32, version: MCVersion) -> i64 {
        let population_seed = self.set_population_seed(world_seed, block_x, block_z, version);
        self.set_decorator_seed_step(population_seed, index, step, version)
    }

    /// `setDecoratorSeed(worldSeed, blockX, blockZ, salt, version)`.
    pub fn set_decorator_seed_at_salt(&mut self, world_seed: i64, block_x: i32, block_z: i32, salt: i32, version: MCVersion) -> i64 {
        let population_seed = self.set_population_seed(world_seed, block_x, block_z, version);
        self.set_decorator_seed_salt(population_seed, salt, version)
    }

    pub fn set_carver_seed(&mut self, world_seed: i64, chunk_x: i32, chunk_z: i32, _version: MCVersion) -> i64 {
        self.set_seed(world_seed);
        let a = self.next_long();
        let b = self.next_long();
        let seed = (chunk_x as i64).wrapping_mul(a) ^ (chunk_z as i64).wrapping_mul(b) ^ world_seed;
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    pub fn set_region_seed(&mut self, world_seed: i64, region_x: i32, region_z: i32, salt: i32, _version: MCVersion) -> i64 {
        let seed = (region_x as i64)
            .wrapping_mul(REGION_SEED_A)
            .wrapping_add((region_z as i64).wrapping_mul(REGION_SEED_B))
            .wrapping_add(world_seed)
            .wrapping_add(salt as i64);
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    pub fn set_weak_seed(&mut self, world_seed: i64, chunk_x: i32, chunk_z: i32, _version: MCVersion) -> i64 {
        let s_x = chunk_x >> 4;
        let s_z = chunk_z >> 4;
        let seed = ((s_x ^ (s_z << 4)) as i64) ^ world_seed;
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    pub fn set_slime_seed(&mut self, world_seed: i64, chunk_x: i32, chunk_z: i32, scrambler: i64, _version: MCVersion) -> i64 {
        let seed = world_seed
            .wrapping_add(chunk_x.wrapping_mul(chunk_x).wrapping_mul(4987142) as i64)
            .wrapping_add(chunk_x.wrapping_mul(5947611) as i64)
            .wrapping_add((chunk_z.wrapping_mul(chunk_z) as i64).wrapping_mul(4392871))
            .wrapping_add(chunk_z.wrapping_mul(389711) as i64)
            ^ scrambler;
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    pub fn set_position_seed(&mut self, x: i32, y: i32, z: i32, _version: MCVersion) -> i64 {
        let seed = position_seed(x, y, z);
        self.set_seed(seed);
        seed & mth::MASK_48
    }

    pub fn set_base_stone_seed(&mut self, world_seed: i64, x: i32, y: i32, z: i32) -> i64 {
        self.set_seed(world_seed);
        let a = self.next_long();
        let b = self.next_long();
        let _c = self.next_long();
        let seed = (x as i64).wrapping_mul(a) ^ (y as i64).wrapping_mul(b) ^ (z as i64).wrapping_mul(z as i64) ^ world_seed;
        self.set_seed(seed);
        seed
    }

    /// `ChunkRand.getInt(minimum, maximum)`.
    pub fn get_int(&mut self, minimum: i32, maximum: i32) -> i32 {
        if minimum >= maximum { minimum } else { self.next_int_bound(maximum - minimum + 1) + minimum }
    }

    /// `ChunkRand.getRandom(list)`.
    pub fn get_random<'a, T>(&mut self, list: &'a [T]) -> &'a T {
        &list[self.next_int_bound(list.len() as i32) as usize]
    }
}

/// `PositionSeed.getPositionSeed`.
pub fn position_seed(x: i32, y: i32, z: i32) -> i64 {
    let mut i = (x.wrapping_mul(3129871) as i64) ^ (z as i64).wrapping_mul(116129781) ^ (y as i64);
    i = i.wrapping_mul(i).wrapping_mul(42317861).wrapping_add(i.wrapping_mul(11));
    i >> 16
}
