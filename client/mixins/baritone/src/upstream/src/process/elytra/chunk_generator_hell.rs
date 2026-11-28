// baritone.process.elytra.pathfinder.ChunkGeneratorHell: the nether's terrain shape for a seed, as Minecraft generated it
// and the native library ported it. The three noise fields do not depend on each other; the Java hands two of them to
// its common pool, here they are computed one after the other.
use super::baritone_elytra_chunk::Chunk;
use super::baritone_elytra_noise::{JavaRandom, NoiseGeneratorOctaves};

pub struct ChunkGeneratorHell {
    lperlin_noise1: NoiseGeneratorOctaves,
    lperlin_noise2: NoiseGeneratorOctaves,
    perlin_noise1: NoiseGeneratorOctaves,
}

const X_SIZE: usize = 5;
const Y_SIZE: usize = 17;
const Z_SIZE: usize = 5;

impl ChunkGeneratorHell {
    pub fn from_seed(seed: i64) -> Self {
        let mut rand = JavaRandom::new(seed);
        // Constructed in this order: they share the one random.
        let a = NoiseGeneratorOctaves::new(&mut rand, 16);
        let b = NoiseGeneratorOctaves::new(&mut rand, 16);
        let c = NoiseGeneratorOctaves::new(&mut rand, 8);
        Self { lperlin_noise1: a, lperlin_noise2: b, perlin_noise1: c }
    }

    /// The terrain of chunk (x, z).
    pub fn generate_chunk(&self, x: i32, z: i32) -> Chunk {
        let mut chunk = Chunk::new();
        self.prepare_heights(x, z, &mut chunk);
        chunk
    }

    fn get_heights(&self, x_offset: i32, y_offset: i32, z_offset: i32) -> Vec<f64> {
        let mut buffer = vec![0.0; X_SIZE * Y_SIZE * Z_SIZE];
        let ar = self.lperlin_noise1.generate_noise_octaves(x_offset, y_offset, z_offset, X_SIZE, Y_SIZE, Z_SIZE, 684.412, 2053.236, 684.412);
        let br = self.lperlin_noise2.generate_noise_octaves(x_offset, y_offset, z_offset, X_SIZE, Y_SIZE, Z_SIZE, 684.412, 2053.236, 684.412);
        let pnr = self.perlin_noise1.generate_noise_octaves(x_offset, y_offset, z_offset, X_SIZE, Y_SIZE, Z_SIZE, 8.555150000000001, 34.2206, 8.555150000000001);

        let mut adouble = [0.0; Y_SIZE];
        for (j, a) in adouble.iter_mut().enumerate() {
            *a = (j as f64 * std::f64::consts::PI * 6.0 / Y_SIZE as f64).cos() * 2.0;
            let mut d2 = j as f64;
            if j > Y_SIZE / 2 {
                d2 = (Y_SIZE - 1 - j) as f64;
            }
            if d2 < 4.0 {
                d2 = 4.0 - d2;
                *a -= d2 * d2 * d2 * 10.0;
            }
        }

        let mut i = 0;
        for _ in 0..X_SIZE {
            for _ in 0..Z_SIZE {
                for k in 0..Y_SIZE {
                    let d4 = adouble[k];
                    let d5 = ar[i] / 512.0;
                    let d6 = br[i] / 512.0;
                    let d7 = (pnr[i] / 10.0 + 1.0) / 2.0;
                    let mut d8 = if d7 < 0.0 {
                        d5
                    } else if d7 > 1.0 {
                        d6
                    } else {
                        d5 + (d6 - d5) * d7
                    };
                    d8 -= d4;
                    if k > Y_SIZE - 4 {
                        let d9 = f64::from((k - (Y_SIZE - 4)) as f32 / 3.0f32);
                        d8 = d8 * (1.0 - d9) + -10.0 * d9;
                    }
                    buffer[i] = d8;
                    i += 1;
                }
            }
        }
        buffer
    }

    fn prepare_heights(&self, x: i32, z: i32, primer: &mut Chunk) {
        let buffer = self.get_heights(x * 4, 0, z * 4);
        let j = 64 / 2 + 1; // 64 = sea level
        let at = |a: usize, b: usize, c: usize| buffer[(a * 5 + b) * 17 + c];

        for j1 in 0..4usize {
            for k1 in 0..4usize {
                for l1 in 0..16usize {
                    let mut d1 = at(j1, k1, l1);
                    let mut d2 = at(j1, k1 + 1, l1);
                    let mut d3 = at(j1 + 1, k1, l1);
                    let mut d4 = at(j1 + 1, k1 + 1, l1);
                    let d5 = (at(j1, k1, l1 + 1) - d1) * 0.125;
                    let d6 = (at(j1, k1 + 1, l1 + 1) - d2) * 0.125;
                    let d7 = (at(j1 + 1, k1, l1 + 1) - d3) * 0.125;
                    let d8 = (at(j1 + 1, k1 + 1, l1 + 1) - d4) * 0.125;

                    for i2 in 0..8usize {
                        let mut d10 = d1;
                        let mut d11 = d2;
                        let d12 = (d3 - d1) * 0.25;
                        let d13 = (d4 - d2) * 0.25;

                        for j2 in 0..4usize {
                            let mut d15 = d10;
                            let d16 = (d11 - d10) * 0.25;

                            for k2 in 0..4usize {
                                // Below the lava sea, or netherrack.
                                if l1 * 8 + i2 < j || d15 > 0.0 {
                                    primer.set_block((j2 + j1 * 4) as i32, (i2 + l1 * 8) as i32, (k2 + k1 * 4) as i32, true);
                                }
                                d15 += d16;
                            }
                            d10 += d12;
                            d11 += d13;
                        }
                        d1 += d5;
                        d2 += d6;
                        d3 += d7;
                        d4 += d8;
                    }
                }
            }
        }
    }
}
