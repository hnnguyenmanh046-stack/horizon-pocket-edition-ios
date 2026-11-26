// Item Physic Lite for Horizon: dropped items lie on the ground instead of
// bobbing and spinning. After Item Physic Lite by CreativeMD; this is the
// pose half, the drawing is `draw.rs`.

pub const ID: &str = "item_physic_lite";
pub const NAME: &str = "Item Physic Lite";
pub const VERSION: &str = "0.1.0";
pub const DESCRIPTION: &str = "Dropped items lie still on the ground.";

/// How thin a model is, along its depth, before it counts as a flat item (a
/// sprite: a sword, an ingot, food) and not a block-like one. Measured in the
/// ground display's units: a sprite is about 0.03 thick there, a block 0.5.
const FLAT_DEPTH: f32 = 0.12;
/// Where a resting item's lowest point sits above the ground.
const REST: f32 = 0.01;

/// Whether a model with these bounds (lo, hi: the corners of its box) is a
/// flat item, which lies down.
pub fn is_flat(lo: [f32; 3], hi: [f32; 3]) -> bool {
    hi[2] - lo[2] < FLAT_DEPTH && hi[1] - lo[1] > hi[2] - lo[2]
}

/// How far up the model is moved so that it rests on the ground. A flat item
/// is turned on its back first (its depth becomes its height), so its lowest
/// point is `lo.z`; a block-like one stands on `lo.y`.
pub fn rest_height(flat: bool, lo: [f32; 3]) -> f32 {
    if flat { REST - lo[2] } else { REST - lo[1] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sprite_is_flat_and_a_block_is_not() {
        assert!(is_flat([-0.25, -0.25, -0.016], [0.25, 0.25, 0.016]));
        assert!(!is_flat([-0.25, -0.25, -0.25], [0.25, 0.25, 0.25]));
    }

    #[test]
    fn items_rest_just_above_the_ground() {
        assert!((rest_height(false, [0.0, -0.25, 0.0]) - 0.26).abs() < 1.0e-6);
        assert!((rest_height(true, [0.0, 0.0, -0.016]) - 0.026).abs() < 1.0e-6);
    }
}
