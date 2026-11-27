// com.seedfinding.mcmath.util.Mth: the bit helpers the LCGs and seeds are built on.

pub const MASK_8: i32 = 0xFF;
pub const MASK_16: i32 = 0xFFFF;
pub const MASK_32: i64 = 0xFFFF_FFFF;
pub const MASK_48: i64 = 0xFFFF_FFFF_FFFF;

pub fn is_power_of_2(value: i64) -> bool {
    (value & value.wrapping_neg()) == value
}

pub fn get_pow2(bits: i32) -> i64 {
    1i64.wrapping_shl(bits as u32)
}

pub fn get_mask(bits: i32) -> i64 {
    if bits >= 64 { !0 } else { get_pow2(bits).wrapping_sub(1) }
}

pub fn mask(value: i64, bits: i32) -> i64 {
    value & get_mask(bits)
}

/// Removes the top bits and copies the sign bit back down.
pub fn mask_signed(value: i64, bits: i32) -> i64 {
    value.wrapping_shl((64 - bits) as u32) >> (64 - bits)
}

pub fn mod_inverse(value: i64) -> i64 {
    mod_inverse_bits(value, 64)
}

pub fn mod_inverse_bits(value: i64, bits: i32) -> i64 {
    let mut x: i64 = ((((value << 1) ^ value) & 4) << 1) ^ value;
    for _ in 0..4 {
        x = x.wrapping_mul(2i64.wrapping_sub(value.wrapping_mul(x)));
    }
    mask(x, bits)
}
