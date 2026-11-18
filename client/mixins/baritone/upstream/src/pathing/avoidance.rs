// baritone.utils.pathing.Avoidance: a sphere the search finds dearer to cross (around a mob or a mob spawner),
// applied as favoring coefficients beside the previous path's.
use super::baritone_settings::Settings;
use super::baritone_utils::Pos;
use std::collections::HashMap;

pub struct Avoidance {
    center: Pos,
    coefficient: f64,
    radius: i32,
}

impl Avoidance {
    pub fn new(center: Pos, coefficient: f64, radius: i32) -> Self {
        Self { center, coefficient, radius }
    }

    /// The coefficient at a position: the avoidance's inside its sphere, 1 outside.
    #[allow(dead_code)]
    pub fn coefficient(&self, p: Pos) -> f64 {
        let d = [p[0] - self.center[0], p[1] - self.center[1], p[2] - self.center[2]];
        if d.iter().map(|v| v * v).sum::<i32>() <= self.radius * self.radius { self.coefficient } else { 1.0 }
    }

    /// `Avoidance.create`: one per mob and per mob spawner worth avoiding, given where they are.
    pub fn create(settings: &Settings, mobs: &dyn Fn() -> Vec<Pos>, spawners: &dyn Fn() -> Vec<Pos>) -> Vec<Avoidance> {
        if !settings.flag("avoidance") {
            return Vec::new();
        }
        let mut res = Vec::new();
        let spawner_coeff = settings.float("mobSpawnerAvoidanceCoefficient");
        let mob_coeff = settings.float("mobAvoidanceCoefficient");
        if spawner_coeff != 1.0 {
            let radius = settings.int("mobSpawnerAvoidanceRadius") as i32;
            res.extend(spawners().into_iter().map(|p| Avoidance::new(p, spawner_coeff, radius)));
        }
        if mob_coeff != 1.0 {
            let radius = settings.int("mobAvoidanceRadius") as i32;
            res.extend(mobs().into_iter().map(|p| Avoidance::new(p, mob_coeff, radius)));
        }
        res
    }

    /// `applySpherical`: multiply the coefficient of every position of the sphere.
    pub fn apply_spherical(&self, map: &mut HashMap<Pos, f64>) {
        let r = self.radius;
        for x in -r..=r {
            for y in -r..=r {
                for z in -r..=r {
                    if x * x + y * y + z * z <= r * r {
                        let p = [self.center[0] + x, self.center[1] + y, self.center[2] + z];
                        *map.entry(p).or_insert(1.0) *= self.coefficient;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sphere_multiplies_what_is_inside_it_and_stacks() {
        let a = Avoidance::new([0, 64, 0], 2.0, 2);
        let mut map = HashMap::new();
        a.apply_spherical(&mut map);
        a.apply_spherical(&mut map);
        assert_eq!(map[&[0, 64, 0]], 4.0);
        assert_eq!(map[&[2, 64, 0]], 4.0);
        assert!(!map.contains_key(&[2, 64, 1]));
        assert_eq!(a.coefficient([3, 64, 0]), 1.0);
    }
}
