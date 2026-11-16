// baritone.api.schematic.mask: which positions of a schematic count. A `Mask` answers for a position and the block that is
// there now; a `StaticMask` for the position alone, so it can be computed once (`PreComputedMask`). Masks combine: not, union,
// intersection and xor (`BooleanBinaryOperators`). `SphereMask` and `CylinderMask` are the shapes `#sel` fills with.
use super::baritone_block_state::BlockState;
use std::rc::Rc;

/// `Mask`.
pub trait Mask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool;
    fn width_x(&self) -> i32;
    fn height_y(&self) -> i32;
    fn length_z(&self) -> i32;
}

/// `StaticMask`: a mask that never looks at the block.
pub trait StaticMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32) -> bool;
    fn width_x(&self) -> i32;
    fn height_y(&self) -> i32;
    fn length_z(&self) -> i32;
}

/// `AbstractMask`: the size every mask keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbstractMask {
    pub width_x: i32,
    pub height_y: i32,
    pub length_z: i32,
}

impl AbstractMask {
    pub fn new(width_x: i32, height_y: i32, length_z: i32) -> Self {
        Self { width_x, height_y, length_z }
    }
}

/// `BooleanBinaryOperators`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanBinaryOperators {
    Or,
    And,
    Xor,
}

impl BooleanBinaryOperators {
    pub fn apply_as_boolean(self, a: bool, b: bool) -> bool {
        match self {
            Self::Or => a || b,
            Self::And => a && b,
            Self::Xor => a ^ b,
        }
    }
}

/// `Mask.not`, `union`, `intersection` and `xor`.
pub trait MaskOps {
    fn not(&self) -> Rc<dyn Mask>;
    fn union(&self, other: &Rc<dyn Mask>) -> Rc<dyn Mask>;
    fn intersection(&self, other: &Rc<dyn Mask>) -> Rc<dyn Mask>;
    fn xor(&self, other: &Rc<dyn Mask>) -> Rc<dyn Mask>;
}

impl MaskOps for Rc<dyn Mask> {
    fn not(&self) -> Rc<dyn Mask> {
        Rc::new(NotMask::new(self.clone()))
    }

    fn union(&self, other: &Rc<dyn Mask>) -> Rc<dyn Mask> {
        Rc::new(BinaryOperatorMask::new(self.clone(), other.clone(), BooleanBinaryOperators::Or))
    }

    fn intersection(&self, other: &Rc<dyn Mask>) -> Rc<dyn Mask> {
        Rc::new(BinaryOperatorMask::new(self.clone(), other.clone(), BooleanBinaryOperators::And))
    }

    fn xor(&self, other: &Rc<dyn Mask>) -> Rc<dyn Mask> {
        Rc::new(BinaryOperatorMask::new(self.clone(), other.clone(), BooleanBinaryOperators::Xor))
    }
}

/// `StaticMask.not`, `union`, `intersection`, `xor` and `compute`.
pub trait StaticMaskOps {
    fn not(&self) -> Rc<dyn StaticMask>;
    fn union(&self, other: &Rc<dyn StaticMask>) -> Rc<dyn StaticMask>;
    fn intersection(&self, other: &Rc<dyn StaticMask>) -> Rc<dyn StaticMask>;
    fn xor(&self, other: &Rc<dyn StaticMask>) -> Rc<dyn StaticMask>;
    fn compute(&self) -> Rc<dyn StaticMask>;
}

impl StaticMaskOps for Rc<dyn StaticMask> {
    fn not(&self) -> Rc<dyn StaticMask> {
        Rc::new(StaticNotMask::new(self.clone()))
    }

    fn union(&self, other: &Rc<dyn StaticMask>) -> Rc<dyn StaticMask> {
        Rc::new(StaticBinaryOperatorMask::new(self.clone(), other.clone(), BooleanBinaryOperators::Or))
    }

    fn intersection(&self, other: &Rc<dyn StaticMask>) -> Rc<dyn StaticMask> {
        Rc::new(StaticBinaryOperatorMask::new(self.clone(), other.clone(), BooleanBinaryOperators::And))
    }

    fn xor(&self, other: &Rc<dyn StaticMask>) -> Rc<dyn StaticMask> {
        Rc::new(StaticBinaryOperatorMask::new(self.clone(), other.clone(), BooleanBinaryOperators::Xor))
    }

    fn compute(&self) -> Rc<dyn StaticMask> {
        Rc::new(PreComputedMask::new(self.as_ref()))
    }
}

/// A static mask used as a mask: it ignores the block.
pub struct AsMask(pub Rc<dyn StaticMask>);

impl Mask for AsMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32, _current: &BlockState) -> bool {
        self.0.part_of_mask(x, y, z)
    }

    fn width_x(&self) -> i32 {
        self.0.width_x()
    }

    fn height_y(&self) -> i32 {
        self.0.height_y()
    }

    fn length_z(&self) -> i32 {
        self.0.length_z()
    }
}

/// `BinaryOperatorMask`.
pub struct BinaryOperatorMask {
    size: AbstractMask,
    a: Rc<dyn Mask>,
    b: Rc<dyn Mask>,
    operator: BooleanBinaryOperators,
}

impl BinaryOperatorMask {
    pub fn new(a: Rc<dyn Mask>, b: Rc<dyn Mask>, operator: BooleanBinaryOperators) -> Self {
        let size = AbstractMask::new(a.width_x().max(b.width_x()), a.height_y().max(b.height_y()), a.length_z().max(b.length_z()));
        Self { size, a, b, operator }
    }

    fn part_of(mask: &dyn Mask, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        x < mask.width_x() && y < mask.height_y() && z < mask.length_z() && mask.part_of_mask(x, y, z, current)
    }
}

impl Mask for BinaryOperatorMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        self.operator.apply_as_boolean(Self::part_of(self.a.as_ref(), x, y, z, current), Self::part_of(self.b.as_ref(), x, y, z, current))
    }

    fn width_x(&self) -> i32 {
        self.size.width_x
    }

    fn height_y(&self) -> i32 {
        self.size.height_y
    }

    fn length_z(&self) -> i32 {
        self.size.length_z
    }
}

/// `BinaryOperatorMask.Static`.
pub struct StaticBinaryOperatorMask {
    size: AbstractMask,
    a: Rc<dyn StaticMask>,
    b: Rc<dyn StaticMask>,
    operator: BooleanBinaryOperators,
}

impl StaticBinaryOperatorMask {
    pub fn new(a: Rc<dyn StaticMask>, b: Rc<dyn StaticMask>, operator: BooleanBinaryOperators) -> Self {
        let size = AbstractMask::new(a.width_x().max(b.width_x()), a.height_y().max(b.height_y()), a.length_z().max(b.length_z()));
        Self { size, a, b, operator }
    }

    fn part_of(mask: &dyn StaticMask, x: i32, y: i32, z: i32) -> bool {
        x < mask.width_x() && y < mask.height_y() && z < mask.length_z() && mask.part_of_mask(x, y, z)
    }
}

impl StaticMask for StaticBinaryOperatorMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32) -> bool {
        self.operator.apply_as_boolean(Self::part_of(self.a.as_ref(), x, y, z), Self::part_of(self.b.as_ref(), x, y, z))
    }

    fn width_x(&self) -> i32 {
        self.size.width_x
    }

    fn height_y(&self) -> i32 {
        self.size.height_y
    }

    fn length_z(&self) -> i32 {
        self.size.length_z
    }
}

/// `NotMask`.
pub struct NotMask {
    source: Rc<dyn Mask>,
}

impl NotMask {
    pub fn new(source: Rc<dyn Mask>) -> Self {
        Self { source }
    }
}

impl Mask for NotMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        !self.source.part_of_mask(x, y, z, current)
    }

    fn width_x(&self) -> i32 {
        self.source.width_x()
    }

    fn height_y(&self) -> i32 {
        self.source.height_y()
    }

    fn length_z(&self) -> i32 {
        self.source.length_z()
    }
}

/// `NotMask.Static`.
pub struct StaticNotMask {
    source: Rc<dyn StaticMask>,
}

impl StaticNotMask {
    pub fn new(source: Rc<dyn StaticMask>) -> Self {
        Self { source }
    }
}

impl StaticMask for StaticNotMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32) -> bool {
        !self.source.part_of_mask(x, y, z)
    }

    fn width_x(&self) -> i32 {
        self.source.width_x()
    }

    fn height_y(&self) -> i32 {
        self.source.height_y()
    }

    fn length_z(&self) -> i32 {
        self.source.length_z()
    }
}

/// `PreComputedMask`: every position asked once, up front. Laid out `[y][z][x]` as in the Java.
pub struct PreComputedMask {
    size: AbstractMask,
    mask: Vec<bool>,
}

impl PreComputedMask {
    pub fn new(source: &dyn StaticMask) -> Self {
        let size = AbstractMask::new(source.width_x(), source.height_y(), source.length_z());
        let mut mask = Vec::with_capacity((size.width_x.max(0) * size.height_y.max(0) * size.length_z.max(0)) as usize);
        for y in 0..size.height_y {
            for z in 0..size.length_z {
                for x in 0..size.width_x {
                    mask.push(source.part_of_mask(x, y, z));
                }
            }
        }
        Self { size, mask }
    }
}

impl StaticMask for PreComputedMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32) -> bool {
        self.mask[((y * self.size.length_z + z) * self.size.width_x + x) as usize]
    }

    fn width_x(&self) -> i32 {
        self.size.width_x
    }

    fn height_y(&self) -> i32 {
        self.size.height_y
    }

    fn length_z(&self) -> i32 {
        self.size.length_z
    }
}

/// `SphereMask`: an ellipsoid as big as the box, solid or only its skin.
pub struct SphereMask {
    size: AbstractMask,
    center: [f64; 3],
    radius_sq: [f64; 3],
    filled: bool,
}

impl SphereMask {
    pub fn new(width_x: i32, height_y: i32, length_z: i32, filled: bool) -> Self {
        let center = [f64::from(width_x) / 2.0, f64::from(height_y) / 2.0, f64::from(length_z) / 2.0];
        Self { size: AbstractMask::new(width_x, height_y, length_z), center, radius_sq: center.map(|c| c * c), filled }
    }

    fn outside(&self, dx: f64, dy: f64, dz: f64) -> bool {
        dx * dx / self.radius_sq[0] + dy * dy / self.radius_sq[1] + dz * dz / self.radius_sq[2] > 1.0
    }
}

impl StaticMask for SphereMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32) -> bool {
        let dx = (f64::from(x) + 0.5 - self.center[0]).abs();
        let dy = (f64::from(y) + 0.5 - self.center[1]).abs();
        let dz = (f64::from(z) + 0.5 - self.center[2]).abs();
        if self.outside(dx, dy, dz) {
            return false;
        }
        self.filled || self.outside(dx + 1.0, dy, dz) || self.outside(dx, dy + 1.0, dz) || self.outside(dx, dy, dz + 1.0)
    }

    fn width_x(&self) -> i32 {
        self.size.width_x
    }

    fn height_y(&self) -> i32 {
        self.size.height_y
    }

    fn length_z(&self) -> i32 {
        self.size.length_z
    }
}

/// `CylinderMask`: along an axis (0 x, 1 y, 2 z). Its radius is one less than half the box, as in the Java.
pub struct CylinderMask {
    size: AbstractMask,
    center_a: f64,
    center_b: f64,
    radius_sq_a: f64,
    radius_sq_b: f64,
    filled: bool,
    alignment: usize,
}

impl CylinderMask {
    pub fn new(width_x: i32, height_y: i32, length_z: i32, filled: bool, alignment: usize) -> Self {
        let center_a = f64::from(Self::get_a(width_x, height_y, alignment)) / 2.0;
        let center_b = f64::from(Self::get_b(height_y, length_z, alignment)) / 2.0;
        Self {
            size: AbstractMask::new(width_x, height_y, length_z),
            center_a,
            center_b,
            radius_sq_a: (center_a - 1.0) * (center_a - 1.0),
            radius_sq_b: (center_b - 1.0) * (center_b - 1.0),
            filled,
            alignment,
        }
    }

    fn outside(&self, da: f64, db: f64) -> bool {
        da * da / self.radius_sq_a + db * db / self.radius_sq_b > 1.0
    }

    fn get_a(x: i32, y: i32, alignment: usize) -> i32 {
        if alignment == 0 { y } else { x }
    }

    fn get_b(y: i32, z: i32, alignment: usize) -> i32 {
        if alignment == 2 { y } else { z }
    }
}

impl StaticMask for CylinderMask {
    fn part_of_mask(&self, x: i32, y: i32, z: i32) -> bool {
        let da = (f64::from(Self::get_a(x, y, self.alignment)) + 0.5 - self.center_a).abs();
        let db = (f64::from(Self::get_b(y, z, self.alignment)) + 0.5 - self.center_b).abs();
        if self.outside(da, db) {
            return false;
        }
        self.filled || self.outside(da + 1.0, db) || self.outside(da, db + 1.0)
    }

    fn width_x(&self) -> i32 {
        self.size.width_x
    }

    fn height_y(&self) -> i32 {
        self.size.height_y
    }

    fn length_z(&self) -> i32 {
        self.size.length_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mask of the positions whose x is below the number, in a 4 by 2 by 2 box.
    struct Left(i32);

    impl StaticMask for Left {
        fn part_of_mask(&self, x: i32, _: i32, _: i32) -> bool {
            x < self.0
        }

        fn width_x(&self) -> i32 {
            4
        }

        fn height_y(&self) -> i32 {
            2
        }

        fn length_z(&self) -> i32 {
            2
        }
    }

    fn solid(mask: &dyn StaticMask) -> Vec<(i32, i32, i32)> {
        let mut found = Vec::new();
        for y in 0..mask.height_y() {
            for z in 0..mask.length_z() {
                for x in 0..mask.width_x() {
                    if mask.part_of_mask(x, y, z) {
                        found.push((x, y, z));
                    }
                }
            }
        }
        found
    }

    #[test]
    fn masks_combine_as_the_java_operators_do() {
        let a: Rc<dyn StaticMask> = Rc::new(Left(2));
        let b: Rc<dyn StaticMask> = Rc::new(Left(3));
        assert_eq!(solid(a.union(&b).as_ref()).len(), 12);
        assert_eq!(solid(a.intersection(&b).as_ref()).len(), 8);
        assert_eq!(solid(a.xor(&b).as_ref()), vec![(2, 0, 0), (2, 0, 1), (2, 1, 0), (2, 1, 1)]);
        assert_eq!(solid(a.not().as_ref()).len(), 8);
        assert_eq!(solid(a.compute().as_ref()), solid(a.as_ref()));
    }

    #[test]
    fn a_position_outside_a_smaller_mask_is_not_part_of_it() {
        struct Small;
        impl StaticMask for Small {
            fn part_of_mask(&self, _: i32, _: i32, _: i32) -> bool {
                true
            }
            fn width_x(&self) -> i32 {
                1
            }
            fn height_y(&self) -> i32 {
                1
            }
            fn length_z(&self) -> i32 {
                1
            }
        }
        let small: Rc<dyn StaticMask> = Rc::new(Small);
        let wide: Rc<dyn StaticMask> = Rc::new(Left(4));
        let both = small.intersection(&wide);
        assert_eq!((both.width_x(), both.height_y(), both.length_z()), (4, 2, 2));
        assert_eq!(solid(both.as_ref()), vec![(0, 0, 0)]);
    }

    #[test]
    fn a_solid_sphere_has_the_cells_inside_the_ellipsoid_and_a_hollow_one_only_its_skin() {
        let sphere = SphereMask::new(5, 5, 5, true);
        assert!(sphere.part_of_mask(2, 2, 2));
        assert!(!sphere.part_of_mask(0, 0, 0));
        let cells = solid(&sphere).len();
        let hollow = SphereMask::new(5, 5, 5, false);
        assert!(!hollow.part_of_mask(2, 2, 2), "the middle is not skin");
        assert!(solid(&hollow).len() < cells);
    }

    #[test]
    fn a_cylinder_is_one_smaller_than_its_box_as_in_the_java() {
        // Along y in a 5 by 5 by 5 box: centre 2.5, radius 1.5, so only the 3 by 3 columns in the middle.
        let cylinder = CylinderMask::new(5, 5, 5, true, 1);
        let columns: Vec<(i32, i32)> = solid(&cylinder).into_iter().filter(|(_, y, _)| *y == 0).map(|(x, _, z)| (x, z)).collect();
        assert_eq!(columns.len(), 9);
        assert!(columns.iter().all(|(x, z)| (1..=3).contains(x) && (1..=3).contains(z)));
        assert!(!cylinder.part_of_mask(0, 0, 2));
    }
}
