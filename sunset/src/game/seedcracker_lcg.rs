// com.seedfinding.mcseed.lcg.LCG: a linear congruential generator, its steps combined into one and inverted.
// The discrete logarithm (`distance`) is only used by the Java's JRand.Debugger, which nothing here needs.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LCG {
    pub multiplier: i64,
    pub addend: i64,
    pub modulus: i64,
    is_power_of_2: bool,
    trailing_zeros: i32,
}

impl LCG {
    pub const CC65_M23: LCG = LCG::new(65793, 4282663, 1 << 23);
    pub const VISUAL_BASIC: LCG = LCG::new(1140671485, 12820163, 1 << 24);
    pub const RTL_UNIFORM: LCG = LCG::new(2147483629, 2147483587, (1 << 31) - 1);
    pub const MINSTD_RAND0_C: LCG = LCG::new(16807, 0, (1 << 31) - 1);
    pub const MINSTD_RAND_C: LCG = LCG::new(48271, 0, (1 << 31) - 1);
    pub const CC65_M31: LCG = LCG::new(16843009, 826366247, 1 << 23);
    pub const RANDU: LCG = LCG::new(65539, 0, 1 << 31);
    pub const GLIB_C: LCG = LCG::new(1103515245, 12345, 1 << 31);
    pub const BORLAND_C: LCG = LCG::new(22695477, 1, 1 << 32);
    pub const PASCAL: LCG = LCG::new(134775813, 1, 1 << 32);
    pub const OPEN_VMS: LCG = LCG::new(69069, 1, 1 << 32);
    pub const NUMERICAL_RECIPES: LCG = LCG::new(1664525, 1013904223, 1 << 32);
    pub const MS_VISUAL_C: LCG = LCG::new(214013, 2531011, 1 << 32);
    pub const JAVA: LCG = LCG::new(25214903917, 11, 1 << 48);
    pub const JAVA_UNIQUIFIER_OLD: LCG = LCG::new(181783497276652981, 0, 0);
    pub const JAVA_UNIQUIFIER_NEW: LCG = LCG::new(1181783497276652981, 0, 0);
    pub const MMIX: LCG = LCG::new(6364136223846793005, 1442695040888963407, 0);
    pub const NEWLIB_C: LCG = LCG::new(6364136223846793005, 1, 0);
    pub const XKCD: LCG = LCG::new(0, 4, 0);

    /// `new LCG(multiplier, addend, modulus)`; a modulus of 0 is 2^64.
    pub const fn new(multiplier: i64, addend: i64, modulus: i64) -> LCG {
        let is_power_of_2 = (modulus & modulus.wrapping_neg()) == modulus;
        LCG { multiplier, addend, modulus, is_power_of_2, trailing_zeros: if is_power_of_2 { modulus.trailing_zeros() as i32 } else { -1 } }
    }

    pub fn is_mod_power_of_2(&self) -> bool {
        self.is_power_of_2
    }

    pub fn mod_trailing_zeroes(&self) -> i32 {
        self.trailing_zeros
    }

    pub fn is_multiplicative(&self) -> bool {
        self.addend == 0
    }

    pub fn next_seed(&self, seed: i64) -> i64 {
        self.modulo(seed.wrapping_mul(self.multiplier).wrapping_add(self.addend))
    }

    pub fn modulo(&self, n: i64) -> i64 {
        if self.is_power_of_2 {
            n & self.modulus.wrapping_sub(1)
        } else if n <= 1i64 << 32 {
            ((n as u64) % (self.modulus as u64)) as i64
        } else {
            panic!("UnsupportedOperationException")
        }
    }

    /// `combine(steps)`: the LCG that does `steps` of this one at once; negative steps go backwards.
    pub fn combine_steps(&self, steps: i64) -> LCG {
        let mut multiplier: i64 = 1;
        let mut addend: i64 = 0;
        let mut intermediate_multiplier = self.multiplier;
        let mut intermediate_addend = self.addend;
        let mut k = steps as u64;
        while k != 0 {
            if k & 1 != 0 {
                multiplier = multiplier.wrapping_mul(intermediate_multiplier);
                addend = intermediate_multiplier.wrapping_mul(addend).wrapping_add(intermediate_addend);
            }
            intermediate_addend = intermediate_multiplier.wrapping_add(1).wrapping_mul(intermediate_addend);
            intermediate_multiplier = intermediate_multiplier.wrapping_mul(intermediate_multiplier);
            k >>= 1;
        }
        multiplier = self.modulo(multiplier);
        addend = self.modulo(addend);
        LCG::new(multiplier, addend, self.modulus)
    }

    /// `combine(lcg)`: this one, then `lcg`.
    pub fn combine(&self, lcg: &LCG) -> LCG {
        assert!(self.modulus == lcg.modulus, "UnsupportedOperationException");
        LCG::new(self.multiplier.wrapping_mul(lcg.multiplier), lcg.multiplier.wrapping_mul(self.addend).wrapping_add(lcg.addend), self.modulus)
    }

    pub fn invert(&self) -> LCG {
        self.combine_steps(-1)
    }
}

/// `LCG.combine(LCG...)`.
pub fn combine_all(lcgs: &[LCG]) -> LCG {
    let mut lcg = lcgs[0];
    for next in &lcgs[1..] {
        lcg = lcg.combine(next);
    }
    lcg
}
