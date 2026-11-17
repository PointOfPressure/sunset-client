// baritone.process.elytra.pathfinder.NoiseGeneratorImproved and NoiseGeneratorOctaves, with the java.util.Random they
// are seeded from: the terrain of a seed has to come out the same bit for bit, so the generator is the JDK's.

const MASK: i64 = (1 << 48) - 1;

/// `java.util.Random`.
pub struct JavaRandom {
    seed: i64,
}

impl JavaRandom {
    pub fn new(seed: i64) -> Self {
        Self { seed: (seed ^ 0x5DEECE66D) & MASK }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    pub fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        let mut r = self.next(31);
        let m = bound - 1;
        if bound & m == 0 {
            // A power of two.
            return ((i64::from(bound) * i64::from(r)) >> 31) as i32;
        }
        let mut u = r;
        r = u % bound;
        while u.wrapping_sub(r).wrapping_add(m) < 0 {
            u = self.next(31);
            r = u % bound;
        }
        r
    }

    pub fn next_double(&mut self) -> f64 {
        let hi = i64::from(self.next(26)) << 27;
        let lo = i64::from(self.next(27));
        (hi + lo) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

const GRAD_X: [f64; 16] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0];
const GRAD_Y: [f64; 16] = [1.0, 1.0, -1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const GRAD_Z: [f64; 16] = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 0.0, 1.0, 0.0, -1.0];

/// Minecraft's improved Perlin noise.
pub struct NoiseGeneratorImproved {
    permutations: [i32; 512],
    x_coord: f64,
    y_coord: f64,
    z_coord: f64,
}

fn lerp(a: f64, b: f64, c: f64) -> f64 {
    b + a * (c - b)
}

fn grad(hash: i32, x: f64, y: f64, z: f64) -> f64 {
    let i = (hash & 15) as usize;
    GRAD_X[i] * x + GRAD_Y[i] * y + GRAD_Z[i] * z
}

/// `(int) d` followed by the floor correction the noise applies everywhere.
fn floor_int(d: f64) -> i32 {
    let i = d as i32;
    if d < f64::from(i) { i - 1 } else { i }
}

fn fade(d: f64) -> f64 {
    d * d * d * (d * (d * 6.0 - 15.0) + 10.0)
}

impl NoiseGeneratorImproved {
    pub fn new(random: &mut JavaRandom) -> Self {
        let x_coord = random.next_double() * 256.0;
        let y_coord = random.next_double() * 256.0;
        let z_coord = random.next_double() * 256.0;
        let mut permutations = [0i32; 512];
        for (i, p) in permutations.iter_mut().enumerate().take(256) {
            *p = i as i32;
        }
        for l in 0..256usize {
            let j = (random.next_int(256 - l as i32) + l as i32) as usize;
            permutations.swap(l, j);
            permutations[l + 256] = permutations[l];
        }
        Self { permutations, x_coord, y_coord, z_coord }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn populate_noise_array(&self, noise: &mut [f64], x_offset: f64, y_offset: f64, z_offset: f64, x_size: usize, y_size: usize, z_size: usize, x_scale: f64, y_scale: f64, z_scale: f64, noise_scale: f64) {
        assert!(y_size != 1, "ySize == 1 uses a code path the native library did not keep");
        let p = &self.permutations;
        let at = |i: i32| p[i as usize];
        let mut i = 0;
        let d0 = 1.0 / noise_scale;
        let mut k = -1;
        let (mut d1, mut d2, mut d3, mut d4) = (0.0, 0.0, 0.0, 0.0);

        for l2 in 0..x_size {
            let mut d5 = x_offset + l2 as f64 * x_scale + self.x_coord;
            let i3 = floor_int(d5);
            let j3 = i3 & 255;
            d5 -= f64::from(i3);
            let d6 = fade(d5);

            for k3 in 0..z_size {
                let mut d7 = z_offset + k3 as f64 * z_scale + self.z_coord;
                let l3 = floor_int(d7);
                let i4 = l3 & 255;
                d7 -= f64::from(l3);
                let d8 = fade(d7);

                for j4 in 0..y_size {
                    let mut d9 = y_offset + j4 as f64 * y_scale + self.y_coord;
                    let k4 = floor_int(d9);
                    let l4 = k4 & 255;
                    d9 -= f64::from(k4);
                    let d10 = fade(d9);

                    if j4 == 0 || l4 != k {
                        k = l4;
                        let l = at(j3) + l4;
                        let i1 = at(l) + i4;
                        let j1 = at(l + 1) + i4;
                        let k1 = at(j3 + 1) + l4;
                        let l1 = at(k1) + i4;
                        let i2 = at(k1 + 1) + i4;
                        d1 = lerp(d6, grad(at(i1), d5, d9, d7), grad(at(l1), d5 - 1.0, d9, d7));
                        d2 = lerp(d6, grad(at(j1), d5, d9 - 1.0, d7), grad(at(i2), d5 - 1.0, d9 - 1.0, d7));
                        d3 = lerp(d6, grad(at(i1 + 1), d5, d9, d7 - 1.0), grad(at(l1 + 1), d5 - 1.0, d9, d7 - 1.0));
                        d4 = lerp(d6, grad(at(j1 + 1), d5, d9 - 1.0, d7 - 1.0), grad(at(i2 + 1), d5 - 1.0, d9 - 1.0, d7 - 1.0));
                    }

                    let d11 = lerp(d10, d1, d2);
                    let d12 = lerp(d10, d3, d4);
                    let d13 = lerp(d8, d11, d12);
                    noise[i] += d13 * d0;
                    i += 1;
                }
            }
        }
    }
}

pub struct NoiseGeneratorOctaves {
    generators: Vec<NoiseGeneratorImproved>,
}

fn lfloor(value: f64) -> i64 {
    let i = value as i64;
    if value < i as f64 { i - 1 } else { i }
}

impl NoiseGeneratorOctaves {
    pub fn new(random: &mut JavaRandom, octaves: usize) -> Self {
        Self { generators: (0..octaves).map(|_| NoiseGeneratorImproved::new(random)).collect() }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate_noise_octaves(&self, x_offset: i32, y_offset: i32, z_offset: i32, x_size: usize, y_size: usize, z_size: usize, x_scale: f64, y_scale: f64, z_scale: f64) -> Vec<f64> {
        let mut noise = vec![0.0; x_size * y_size * z_size];
        let mut d3 = 1.0;
        for generator in &self.generators {
            let mut d0 = f64::from(x_offset) * d3 * x_scale;
            let d1 = f64::from(y_offset) * d3 * y_scale;
            let mut d2 = f64::from(z_offset) * d3 * z_scale;
            let mut k = lfloor(d0);
            let mut l = lfloor(d2);
            d0 -= k as f64;
            d2 -= l as f64;
            k %= 16_777_216;
            l %= 16_777_216;
            d0 += k as f64;
            d2 += l as f64;
            generator.populate_noise_array(&mut noise, d0, d1, d2, x_size, y_size, z_size, x_scale * d3, y_scale * d3, z_scale * d3, d3);
            d3 /= 2.0;
        }
        noise
    }
}
