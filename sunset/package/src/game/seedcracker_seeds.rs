// com.seedfinding.mccore.rand.seed: PillarSeed, StructureSeed, WorldSeed, RegionSeed, PositionSeed and SeedMixer, with
// NextLongReverser and SeedIterator. What the Java keeps for string seeds (StringUnhasher) is left out: nothing here asks
// whether a seed is the hash of a text.
use super::sunset_scx_lcg::LCG;
use super::sunset_scx_mth as mth;
use super::sunset_scx_rand::JRand;
use super::sunset_scx_sha256 as sha256;

/// `SeedIterator`: the seeds from `min` up to (not including) `max`, each one mapped.
pub struct SeedIterator {
    min: i64,
    max: i64,
    seed: i64,
    mapper: Box<dyn Fn(i64) -> i64 + Send + Sync>,
}

impl SeedIterator {
    pub fn new(min: i64, max: i64) -> SeedIterator {
        Self::with_mapper(min, max, |s| s)
    }

    pub fn with_mapper(min: i64, max: i64, mapper: impl Fn(i64) -> i64 + Send + Sync + 'static) -> SeedIterator {
        SeedIterator { min, max, seed: min, mapper: Box::new(mapper) }
    }

    pub fn has_next(&self) -> bool {
        self.seed < self.max
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> i64 {
        let s = self.seed;
        self.seed += 1;
        (self.mapper)(s)
    }

    pub fn for_each_remaining(&mut self, mut action: impl FnMut(i64)) {
        while self.has_next() {
            let s = self.next();
            action(s);
        }
    }

    /// `asStream`: every seed from `min`, mapped, ignoring how far the iterator has come.
    pub fn as_stream(&self) -> impl Iterator<Item = i64> + '_ {
        (self.min..self.max).map(move |s| (self.mapper)(s))
    }
}

pub mod pillar_seed {
    use super::*;

    pub fn from_structure_seed(structure_seed: i64) -> i64 {
        (LCG::JAVA.combine_steps(2).next_seed(structure_seed ^ LCG::JAVA.multiplier) >> 16) & mth::MASK_16 as i64
    }

    pub fn get_structure_seeds(pillar_seed: i64) -> SeedIterator {
        let back2 = LCG::JAVA.combine_steps(-2);
        SeedIterator::with_mapper(0, 1 << 32, move |partial| {
            let mut current = (partial & (mth::MASK_32 - mth::MASK_16 as i64)) << 16;
            current |= partial & mth::MASK_16 as i64;
            current |= pillar_seed << 16;
            current = back2.next_seed(current);
            current ^ LCG::JAVA.multiplier
        })
    }

    pub fn get_pillar_heights(pillar_seed: i64) -> [i32; 10] {
        let mut heights = [0i32; 10];
        for (i, h) in heights.iter_mut().enumerate() {
            *h = 76 + i as i32 * 3;
        }
        let mut rand = JRand::new(pillar_seed);
        let mut i = heights.len();
        while i > 1 {
            let a = i - 1;
            let b = rand.next_int_bound(i as i32) as usize;
            heights.swap(a, b);
            i -= 1;
        }
        heights
    }

    pub fn from_pillar_heights(heights: &[i32]) -> Vec<i64> {
        let mut seeds = Vec::new();
        let mut it = iterator();
        it.for_each_remaining(|pillar_seed| {
            if get_pillar_heights(pillar_seed)[..] == *heights {
                seeds.push(pillar_seed);
            }
        });
        seeds
    }

    pub fn iterator() -> SeedIterator {
        SeedIterator::new(0, 1 << 16)
    }
}

pub mod structure_seed {
    use super::*;

    pub fn to_pillar_seed(structure_seed: i64) -> i64 {
        pillar_seed::from_structure_seed(structure_seed)
    }

    pub fn to_world_seed(structure_seed: i64, upper_bits: i64) -> i64 {
        (upper_bits << 48) | world_seed::to_structure_seed(structure_seed)
    }

    pub fn to_random_world_seeds(structure_seed: i64) -> Vec<i64> {
        next_long_reverser::get_next_long_equivalents(structure_seed)
    }

    pub fn get_world_seeds(structure_seed: i64) -> SeedIterator {
        SeedIterator::with_mapper(0, 1 << 16, move |upper| to_world_seed(structure_seed, upper))
    }

    pub fn iterator() -> SeedIterator {
        SeedIterator::new(0, 1 << 48)
    }
}

pub mod world_seed {
    use super::*;

    pub fn to_pillar_seed(world_seed: i64) -> i64 {
        structure_seed::to_pillar_seed(world_seed)
    }

    pub fn is_structure_seed(world_seed: i64) -> bool {
        to_structure_seed(world_seed) == world_seed
    }

    pub fn to_structure_seed(world_seed: i64) -> i64 {
        world_seed & mth::MASK_48
    }

    pub fn get_shadow_seed(world_seed: i64) -> i64 {
        seed_mixer::get_other_solution(world_seed)
    }

    pub fn get_sister_seeds(world_seed: i64) -> SeedIterator {
        structure_seed::get_world_seeds(to_structure_seed(world_seed))
    }

    pub fn is_string(world_seed: i64) -> bool {
        world_seed as i32 as i64 == world_seed
    }

    pub fn is_random(world_seed: i64) -> bool {
        let upper_bits = ((world_seed as u64) >> 32) as i64;
        let lower_bits = world_seed & mth::MASK_32;
        let a = (24667315i64.wrapping_mul(upper_bits).wrapping_add(18218081i64.wrapping_mul(lower_bits)).wrapping_add(67552711)) >> 32;
        let b = ((-4824621i64).wrapping_mul(upper_bits).wrapping_add(7847617i64.wrapping_mul(lower_bits)).wrapping_add(7847617)) >> 32;
        let seed = 7847617i64.wrapping_mul(a).wrapping_sub(18218081i64.wrapping_mul(b));
        JRand::next_long_at(seed) == world_seed
    }

    /// `fromHash`: the world seeds of a structure seed whose hash is the one the server sent.
    pub fn from_hash(structure_seed: i64, hashed_world_seed: i64) -> Vec<i64> {
        from_hash_cancellable(structure_seed, hashed_world_seed, || false)
    }

    pub fn from_hash_cancellable(structure_seed: i64, hashed_world_seed: i64, cancelled: impl Fn() -> bool) -> Vec<i64> {
        let mut world_seeds = Vec::new();
        for upper in 0..1 << 16 {
            if (upper & 255) == 0 && cancelled() {
                return Vec::new();
            }
            let world_seed = structure_seed::to_world_seed(structure_seed, upper);
            if to_hash(world_seed) == hashed_world_seed {
                world_seeds.push(world_seed);
            }
        }
        if cancelled() {
            return Vec::new();
        }
        if world_seeds.len() > 1 {
            eprintln!(
                "This should never happen. Open a github issue right now and provide the structure seed [{structure_seed}] as well as the hashed seed [{hashed_world_seed}], you have stumbled upon an incredibly rare collision."
            );
        }
        world_seeds
    }

    /// `toHash`: the first eight bytes, little endian, of the SHA-256 of the seed's eight bytes, little endian.
    pub fn to_hash(world_seed: i64) -> i64 {
        let mut seed = world_seed as u64;
        let mut bytes = [0u8; 8];
        for b in bytes.iter_mut() {
            *b = (seed & 0xFF) as u8;
            seed >>= 8;
        }
        let digest = sha256::digest(&bytes);
        let mut hashed = digest[0] as u64;
        for (i, b) in digest.iter().enumerate().take(8).skip(1) {
            hashed |= (*b as u64) << (i << 3);
        }
        hashed as i64
    }

    pub fn random_seeds_iterator() -> SeedIterator {
        SeedIterator::with_mapper(0, 1 << 48, JRand::next_long_at)
    }
}

pub mod region_seed {
    use super::super::sunset_scx_rand::{REGION_SEED_A, REGION_SEED_B};

    pub const A: i64 = REGION_SEED_A;
    pub const B: i64 = REGION_SEED_B;

    pub fn get_increment(d_region_x: i32, d_region_z: i32) -> i64 {
        A.wrapping_mul(d_region_x as i64).wrapping_add(B.wrapping_mul(d_region_z as i64))
    }

    pub fn to_world_seed(region_seed: i64, region_x: i32, region_z: i32, salt: i32) -> i64 {
        translate(region_seed, region_x, region_z).wrapping_sub(salt as i64)
    }

    pub fn translate(region_seed: i64, d_region_x: i32, d_region_z: i32) -> i64 {
        region_seed.wrapping_sub(get_increment(d_region_x, d_region_z))
    }

    pub fn get_neighbor(region_seed: i64, d_region_x: i32, d_region_z: i32) -> i64 {
        region_seed.wrapping_add(get_increment(d_region_x, d_region_z))
    }

    pub fn are_neighbors(region_seed1: i64, region_seed2: i64, d_region_x: i32, d_region_z: i32) -> bool {
        are_neighbors_salted(region_seed1, region_seed2, 0, 0, d_region_x, d_region_z)
    }

    pub fn are_neighbors_salted(region_seed1: i64, region_seed2: i64, salt1: i32, salt2: i32, d_region_x: i32, d_region_z: i32) -> bool {
        get_increment(d_region_x, d_region_z) == (region_seed2.wrapping_sub(salt2 as i64)).wrapping_sub(region_seed1.wrapping_sub(salt1 as i64))
    }
}

pub mod seed_mixer {
    use super::*;

    pub const A: i64 = LCG::MMIX.multiplier;
    pub const B: i64 = LCG::MMIX.addend;

    /// `MAGIC = -B * modInverse(A)`.
    pub fn magic() -> i64 {
        B.wrapping_neg().wrapping_mul(mth::mod_inverse(A))
    }

    pub fn get_other_solution(seed: i64) -> i64 {
        magic().wrapping_sub(seed)
    }

    pub fn mix_seed(seed: i64, salt: i64) -> i64 {
        let seed = seed.wrapping_mul(seed.wrapping_mul(A).wrapping_add(B));
        seed.wrapping_add(salt)
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Solution {
        Even,
        Odd,
    }

    impl Solution {
        pub fn of(n: i64) -> Solution {
            if n & 1 == 0 { Solution::Even } else { Solution::Odd }
        }
    }

    pub fn unmix_seed(seed: i64, salt: i64, solution: Solution) -> i64 {
        assert!(((seed.wrapping_sub(salt)) & 1) != 1, "Seed {seed} is unreachable with salt {salt}");
        let mut r: i64 = solution as i64;
        let mut j = 1;
        while j < 64 {
            let f = A.wrapping_mul(r).wrapping_mul(r).wrapping_add(B.wrapping_mul(r)).wrapping_add(salt).wrapping_sub(seed);
            r = r.wrapping_sub(f.wrapping_mul(mth::mod_inverse_bits(2i64.wrapping_mul(A).wrapping_mul(r).wrapping_add(B), 64)));
            j <<= 1;
        }
        r
    }
}

pub mod next_long_reverser {
    use super::*;

    /// `addSeedsToList`: the internal seeds a `nextLong` could start from to give a structure seed's 48 bits.
    pub fn add_seeds_to_list(structure_seed: i64, seed_list: &mut Vec<i64>) {
        let lower_bits = structure_seed & 0xffff_ffff;
        let mut upper_bits = ((structure_seed as u64) >> 32) as i64;
        if (lower_bits & 0x8000_0000) != 0 {
            upper_bits += 1; // restoring the initial value of the upper bits
        }
        let bits_of_danger = 1;
        let low_min = lower_bits << (16 - bits_of_danger);
        let low_max = ((lower_bits + 1) << (16 - bits_of_danger)) - 1;
        let upper_min = ((upper_bits << 16) - 107048004364969) >> bits_of_danger;
        let m1lv = (low_max * -33441 + upper_min * 17549).div_euclid(1i64 << (31 - bits_of_danger)) + 1; // a common factor of 2 cancelled out
        let m2lv = (low_min * 46603 + upper_min * 39761).div_euclid(1i64 << (32 - bits_of_danger)) + 1;
        let pull_back = |seed: i64| (254681119335897i64.wrapping_mul(seed).wrapping_add(120305458776662)) & 0xffff_ffff_ffff;
        let upper_matches = |a: i64, b: i64| (((46603 * a + 66882 * b) + 107048004364969) as u64 >> 16) as i64 == upper_bits;
        let lower_matches = |seed: i64| ((seed as u64) >> 16) as i64 == lower_bits;

        let seed = -39761 * m1lv + 35098 * m2lv;
        if upper_matches(m1lv, m2lv) && lower_matches(seed) {
            seed_list.push(pull_back(seed));
        }
        let seed = -39761 * (m1lv + 1) + 35098 * m2lv;
        if upper_matches(m1lv + 1, m2lv) && lower_matches(seed) {
            seed_list.push(pull_back(seed));
        }
        let seed = -39761 * m1lv + 35098 * (m2lv + 1);
        if upper_matches(m1lv, m2lv + 1) && lower_matches(seed) {
            seed_list.push(pull_back(seed));
        }
    }

    pub fn get_seeds(structure_seed: i64) -> Vec<i64> {
        let mut seeds = Vec::with_capacity(2);
        add_seeds_to_list(structure_seed, &mut seeds);
        seeds
    }

    pub fn add_next_long_equivalents(structure_seed: i64, next_longs: &mut Vec<i64>) {
        for seed in get_seeds(structure_seed) {
            next_longs.push(JRand::with_scramble(seed, false).next_long());
        }
    }

    pub fn get_next_long_equivalents(structure_seed: i64) -> Vec<i64> {
        let mut next_longs = Vec::with_capacity(2);
        add_next_long_equivalents(structure_seed, &mut next_longs);
        next_longs
    }
}
