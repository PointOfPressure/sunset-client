// net.minecraft.world.level.levelgen.WorldgenRandom over XoroshiroRandomSource, as the world generator of 1.18 and later
// seeds a chunk's decoration: setDecorationSeed, then setFeatureSeed. The legacy seed is first widened to 128 bits
// (RandomSupport.upgradeSeedTo128bit) and then drives Xoroshiro128++. `nextInt(bound)` is the bounded draw of
// XoroshiroRandomSource (Lemire's method), not the one of java.util.Random.

const GOLDEN_RATIO_64: i64 = -7046029254386353131; // 0x9e3779b97f4a7c15
const SILVER_RATIO_64: i64 = 7640891576956012809; // 0x6a09e667f3bcc909

fn mix_stafford13(mut z: i64) -> i64 {
    z = (z ^ ((z as u64) >> 30) as i64).wrapping_mul(-4658895280553007687);
    z = (z ^ ((z as u64) >> 27) as i64).wrapping_mul(-7723592293110705685);
    z ^ ((z as u64) >> 31) as i64
}

/// `RandomSupport.upgradeSeedTo128bit(long)`: the two halves of the 128-bit state.
pub fn upgrade_seed_to_128bit(seed: i64) -> (i64, i64) {
    let l = seed ^ SILVER_RATIO_64;
    let m = l.wrapping_add(GOLDEN_RATIO_64);
    (mix_stafford13(l), mix_stafford13(m))
}

#[derive(Clone, Debug)]
pub struct Xoroshiro128PlusPlus {
    seed_lo: i64,
    seed_hi: i64,
}

impl Xoroshiro128PlusPlus {
    pub fn new(seed_lo: i64, seed_hi: i64) -> Self {
        if (seed_lo | seed_hi) == 0 {
            return Self { seed_lo: GOLDEN_RATIO_64, seed_hi: SILVER_RATIO_64 };
        }
        Self { seed_lo, seed_hi }
    }

    pub fn next_long(&mut self) -> i64 {
        let l = self.seed_lo;
        let mut m = self.seed_hi;
        let n = l.wrapping_add(m).rotate_left(17).wrapping_add(l);
        m ^= l;
        self.seed_lo = l.rotate_left(49) ^ m ^ (m << 21);
        self.seed_hi = m.rotate_left(28);
        n
    }
}

/// `WorldgenRandom(new XoroshiroRandomSource(seed))`.
#[derive(Clone, Debug)]
pub struct WorldgenRandom {
    generator: Xoroshiro128PlusPlus,
}

impl WorldgenRandom {
    pub fn new(seed: i64) -> WorldgenRandom {
        let (lo, hi) = upgrade_seed_to_128bit(seed);
        WorldgenRandom { generator: Xoroshiro128PlusPlus::new(lo, hi) }
    }

    pub fn set_seed(&mut self, seed: i64) {
        let (lo, hi) = upgrade_seed_to_128bit(seed);
        self.generator = Xoroshiro128PlusPlus::new(lo, hi);
    }

    pub fn next_long(&mut self) -> i64 {
        self.generator.next_long()
    }

    pub fn next_int(&mut self) -> i32 {
        self.generator.next_long() as i32
    }

    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "Bound must be positive");
        let mut l = self.next_int() as u32 as u64;
        let mut m = l * bound as u64;
        let mut n = m & 0xFFFF_FFFF;
        if n < bound as u64 {
            let threshold = (!(bound as u32)).wrapping_add(1) % bound as u32;
            while n < threshold as u64 {
                l = self.next_int() as u32 as u64;
                m = l * bound as u64;
                n = m & 0xFFFF_FFFF;
            }
        }
        (m >> 32) as i32
    }

    /// `setDecorationSeed(worldSeed, minBlockX, minBlockZ)`: the population seed of the chunk.
    pub fn set_decoration_seed(&mut self, world_seed: i64, x: i32, z: i32) -> i64 {
        self.set_seed(world_seed);
        let m = self.next_long() | 1;
        let n = self.next_long() | 1;
        let o = (x as i64).wrapping_mul(m).wrapping_add((z as i64).wrapping_mul(n)) ^ world_seed;
        self.set_seed(o);
        o
    }

    /// `setFeatureSeed(decorationSeed, index, step)`.
    pub fn set_feature_seed(&mut self, decoration_seed: i64, index: i32, step: i32) {
        let m = decoration_seed.wrapping_add(index as i64).wrapping_add((10000 * step) as i64);
        self.set_seed(m);
    }
}
