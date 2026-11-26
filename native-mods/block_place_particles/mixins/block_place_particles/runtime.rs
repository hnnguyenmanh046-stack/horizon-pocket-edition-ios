// Block Place Particles: where the particles of a placed block start and which
// way they fly. This file is both the standalone library and the module
// injected into `Game`; the game half lives in `place.rs`.
//
// Ported from Particle Interactions (games.enchanted), `spawnBlockPlaceParticle`:
// particles along the edges of the block's shape, none on the bottom edges when
// a solid block is under it, none on the top edges when one is over it.

pub const ID: &str = "block_place_particles";
pub const NAME: &str = "Block Place Particles";
pub const VERSION: &str = "0.1.1";
pub const DESCRIPTION: &str = "Placing a block sends particles off its edges.";

/// Particles along each of the 12 edges.
pub const PER_EDGE: usize = 2;
/// How fast a particle leaves the block's centre, in blocks per tick.
const OUTWARD: f64 = 0.05;

/// Blocks with no terrain particles of their own: air, fluids and the
/// technical blocks.
pub fn skips(name: &str) -> bool {
    matches!(
        name,
        "minecraft:water"
            | "minecraft:lava"
            | "minecraft:barrier"
            | "minecraft:light"
            | "minecraft:structure_void"
            | "minecraft:moving_piston"
    ) || name.ends_with("air")
}

/// The box around all of a shape's boxes (`[min x, min y, min z, max x, max y,
/// max z]`, in the block), or None for an empty or broken shape.
pub fn bounds(boxes: &[[f64; 6]]) -> Option<[f64; 6]> {
    let first = boxes.first()?;
    let mut out = *first;
    for b in boxes {
        for i in 0..3 {
            out[i] = out[i].min(b[i]);
            out[i + 3] = out[i + 3].max(b[i + 3]);
        }
    }
    let sound = out.iter().all(|v| v.is_finite()) && (0..3).all(|i| out[i + 3] > out[i]);
    sound.then_some(out)
}

/// `(offset in the block, velocity)` of every particle: `per_edge` along each
/// edge of `bounds`, flying away from its centre. The bottom edges are left
/// out under a solid block (`solid_below`), the top ones under another
/// (`solid_above`).
pub fn edge_points(
    bounds: [f64; 6],
    per_edge: usize,
    solid_below: bool,
    solid_above: bool,
) -> Vec<([f64; 3], [f64; 3])> {
    let centre = [
        (bounds[0] + bounds[3]) / 2.0,
        (bounds[1] + bounds[4]) / 2.0,
        (bounds[2] + bounds[5]) / 2.0,
    ];
    let mut out = Vec::new();
    // An edge runs along `axis`; the other two coordinates sit at a min or a max.
    for axis in 0..3 {
        let (u, v) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        for hi_u in [false, true] {
            for hi_v in [false, true] {
                // The edge's height: y is `u` or `v` when the edge is not the
                // vertical one (axis 1).
                let y_hi = if axis == 1 {
                    None
                } else if u == 1 {
                    Some(hi_u)
                } else {
                    Some(hi_v)
                };
                match y_hi {
                    Some(false) if solid_below => continue,
                    Some(true) if solid_above => continue,
                    _ => {}
                }
                for i in 0..per_edge {
                    let t = (i as f64 + 0.5) / per_edge as f64;
                    let mut p = [0.0; 3];
                    p[axis] = bounds[axis] + (bounds[axis + 3] - bounds[axis]) * t;
                    p[u] = if hi_u { bounds[u + 3] } else { bounds[u] };
                    p[v] = if hi_v { bounds[v + 3] } else { bounds[v] };
                    let d = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
                    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    let vel = if len > 0.0 {
                        [d[0] / len * OUTWARD, d[1] / len * OUTWARD, d[2] / len * OUTWARD]
                    } else {
                        [0.0; 3]
                    };
                    out.push((p, vel));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

    #[test]
    fn a_free_cube_has_twelve_edges() {
        assert_eq!(edge_points(CUBE, 2, false, false).len(), 12 * 2);
    }

    #[test]
    fn solid_neighbours_drop_their_edges() {
        assert_eq!(edge_points(CUBE, 2, true, false).len(), 8 * 2);
        assert_eq!(edge_points(CUBE, 2, true, true).len(), 4 * 2);
    }

    #[test]
    fn particles_leave_the_centre() {
        for (p, v) in edge_points(CUBE, 2, false, false) {
            let away = (p[0] - 0.5) * v[0] + (p[1] - 0.5) * v[1] + (p[2] - 0.5) * v[2];
            assert!(away > 0.0, "{p:?} {v:?}");
        }
    }

    #[test]
    fn air_and_fluids_are_skipped() {
        assert!(skips("minecraft:air") && skips("minecraft:cave_air"));
        assert!(skips("minecraft:water") && !skips("minecraft:stone"));
    }

    #[test]
    fn bounds_wrap_every_box_and_reject_nothing_shapes() {
        assert_eq!(bounds(&[]), None);
        assert_eq!(
            bounds(&[[0.0, 0.0, 0.0, 0.5, 1.0, 1.0], [0.5, 0.0, 0.0, 1.0, 0.5, 1.0]]),
            Some(CUBE)
        );
        assert_eq!(bounds(&[[0.0, 0.0, 0.0, 0.0, 1.0, 1.0]]), None);
    }
}
