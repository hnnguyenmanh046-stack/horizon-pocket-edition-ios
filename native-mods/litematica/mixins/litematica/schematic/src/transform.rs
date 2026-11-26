//! Where a placed schematic's blocks go, and how their states turn with it:
//! mirrored first, then turned clockwise (seen from above) about its origin.

use crate::Block;

/// Mirroring: none, flip east-west (x), flip north-south (z).
pub const MIRRORS: [&str; 3] = ["None", "East-West", "North-South"];
pub const ROTATIONS: [&str; 4] = ["0°", "90°", "180°", "270°"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Placement {
    pub origin: [i32; 3],
    /// Quarter turns clockwise, 0 to 3.
    pub rotation: u8,
    /// One of `MIRRORS`.
    pub mirror: u8,
}

impl Placement {
    /// Where `p` (relative to the schematic's origin) is in the world.
    pub fn world(&self, p: [i32; 3]) -> [i32; 3] {
        let [mut x, y, mut z] = p;
        match self.mirror {
            1 => x = -x,
            2 => z = -z,
            _ => {}
        }
        let (x, z) = match self.rotation % 4 {
            1 => (-z, x),
            2 => (-x, -z),
            3 => (z, -x),
            _ => (x, z),
        };
        [self.origin[0] + x, self.origin[1] + y, self.origin[2] + z]
    }

    /// Where world position `w` is relative to the schematic's origin (the
    /// inverse of `world`).
    pub fn local(&self, w: [i32; 3]) -> [i32; 3] {
        let (x, y, z) = (w[0] - self.origin[0], w[1] - self.origin[1], w[2] - self.origin[2]);
        // Undo the turn: (x, z) of a quarter turn back is (z, -x).
        let (mut x, mut z) = match self.rotation % 4 {
            1 => (z, -x),
            2 => (-x, -z),
            3 => (-z, x),
            _ => (x, z),
        };
        match self.mirror {
            1 => x = -x,
            2 => z = -z,
            _ => {}
        }
        [x, y, z]
    }

    /// The world box of a schematic box `lo..=hi` (relative to its origin).
    pub fn world_box(&self, lo: [i32; 3], hi: [i32; 3]) -> ([i32; 3], [i32; 3]) {
        let a = self.world(lo);
        let b = self.world(hi);
        (std::array::from_fn(|k| a[k].min(b[k])), std::array::from_fn(|k| a[k].max(b[k])))
    }

    /// A horizontal direction after the mirror and the turn.
    pub fn direction(&self, d: &str) -> String {
        let mut d = d.to_owned();
        match (self.mirror, d.as_str()) {
            (1, "east") => d = "west".into(),
            (1, "west") => d = "east".into(),
            (2, "north") => d = "south".into(),
            (2, "south") => d = "north".into(),
            _ => {}
        }
        const CW: [&str; 4] = ["north", "east", "south", "west"];
        if let Some(i) = CW.iter().position(|c| *c == d) {
            d = CW[(i + self.rotation as usize) % 4].into();
        }
        d
    }

    fn flips(&self) -> bool {
        self.mirror != 0
    }

    /// `b` as it stands once placed: facings, axes, sign rotations,
    /// connections, rail and stair shapes, door hinges and chest halves.
    pub fn block(&self, b: &Block) -> Block {
        if self.rotation % 4 == 0 && self.mirror == 0 {
            return b.clone();
        }
        let mut out = b.clone();
        let old = b.clone();
        for (key, value) in &old.props {
            let new = match key.as_str() {
                "facing" | "horizontal_facing" => self.direction(value),
                "axis" if self.rotation % 2 == 1 => match value.as_str() {
                    "x" => "z".into(),
                    "z" => "x".into(),
                    v => v.into(),
                },
                "rotation" => match value.parse::<i32>() {
                    Ok(r) => {
                        let mut r = r;
                        match self.mirror {
                            1 => r = (16 - r) % 16,
                            2 => r = (8 - r).rem_euclid(16),
                            _ => {}
                        }
                        ((r + 4 * self.rotation as i32) % 16).to_string()
                    }
                    Err(_) => value.clone(),
                },
                "shape" if b.short().contains("rail") => self.rail(value),
                "shape" if self.flips() => match value.as_str() {
                    "inner_left" => "inner_right".into(),
                    "inner_right" => "inner_left".into(),
                    "outer_left" => "outer_right".into(),
                    "outer_right" => "outer_left".into(),
                    v => v.into(),
                },
                "hinge" if self.flips() => match value.as_str() {
                    "left" => "right".into(),
                    "right" => "left".into(),
                    v => v.into(),
                },
                "type" if self.flips() && (b.short().ends_with("chest")) => match value.as_str() {
                    "left" => "right".into(),
                    "right" => "left".into(),
                    v => v.into(),
                },
                _ => continue,
            };
            out.set(key, &new);
        }
        // Connections (fences, walls, glass panes, redstone, vines...): each
        // side's value moves to where that side went.
        let sides = ["north", "east", "south", "west"];
        if sides.iter().any(|s| old.prop(s).is_some()) {
            for s in sides {
                if let Some(v) = old.prop(s) {
                    out.set(&self.direction(s), v);
                }
            }
        }
        out
    }

    /// A rail's shape (`north_south`, `ascending_east`, `south_west`...) turned.
    fn rail(&self, shape: &str) -> String {
        if let Some(d) = shape.strip_prefix("ascending_") {
            return format!("ascending_{}", self.direction(d));
        }
        let parts: Vec<String> = shape.split('_').map(|d| self.direction(d)).collect();
        if parts.len() != 2 {
            return shape.into();
        }
        let ns = |d: &str| d == "north" || d == "south";
        if ns(&parts[0]) && ns(&parts[1]) {
            return "north_south".into();
        }
        if !ns(&parts[0]) && !ns(&parts[1]) {
            return "east_west".into();
        }
        // Corners name north/south first.
        let (a, b) = if ns(&parts[0]) { (&parts[0], &parts[1]) } else { (&parts[1], &parts[0]) };
        format!("{a}_{b}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_turn_clockwise_about_the_origin() {
        let p = Placement { origin: [100, 64, 200], rotation: 1, mirror: 0 };
        // East of the origin (+x) turns to south (+z).
        assert_eq!(p.world([1, 0, 0]), [100, 64, 201]);
        // South (+z) turns to west (-x).
        assert_eq!(p.world([0, 0, 1]), [99, 64, 200]);
        let p = Placement { origin: [0, 0, 0], rotation: 2, mirror: 0 };
        assert_eq!(p.world([3, 5, 2]), [-3, 5, -2]);
        let p = Placement { origin: [0, 0, 0], rotation: 0, mirror: 1 };
        assert_eq!(p.world([3, 5, 2]), [-3, 5, 2]);
        let p = Placement { origin: [0, 0, 0], rotation: 4 + 3, mirror: 2 };
        // Mirror z first: (3, 5, -2); then 270°: (x, z) -> (z, -x).
        assert_eq!(p.world([3, 5, 2]), [-2, 5, -3]);
    }

    #[test]
    fn local_undoes_world() {
        for rotation in 0..4 {
            for mirror in 0..3 {
                let p = Placement { origin: [7, -3, 11], rotation, mirror };
                for q in [[0, 0, 0], [3, 5, 2], [-4, 1, 9], [1, -2, -6]] {
                    assert_eq!(p.local(p.world(q)), q, "rotation {rotation} mirror {mirror}");
                }
            }
        }
    }

    #[test]
    fn four_quarter_turns_come_back() {
        let b = Block::new("minecraft:oak_stairs", &[("facing", "north"), ("shape", "inner_left"), ("half", "top")]);
        let mut cur = b.clone();
        let turn = Placement { rotation: 1, ..Default::default() };
        let mut seen = Vec::new();
        for _ in 0..4 {
            cur = turn.block(&cur);
            seen.push(cur.prop("facing").unwrap().to_owned());
        }
        assert_eq!(seen, ["east", "south", "west", "north"]);
        assert_eq!(cur, b);
    }

    #[test]
    fn states_follow_the_mirror() {
        let m = Placement { mirror: 1, ..Default::default() };
        let stairs = Block::new("minecraft:stone_stairs", &[("facing", "east"), ("shape", "outer_left")]);
        let s = m.block(&stairs);
        assert_eq!(s.prop("facing"), Some("west"));
        assert_eq!(s.prop("shape"), Some("outer_right"));
        let door = Block::new("minecraft:oak_door", &[("facing", "north"), ("hinge", "left")]);
        let d = m.block(&door);
        assert_eq!(d.prop("facing"), Some("north"));
        assert_eq!(d.prop("hinge"), Some("right"));
        let sign = Block::new("minecraft:oak_sign", &[("rotation", "4")]);
        assert_eq!(m.block(&sign).prop("rotation"), Some("12"));
        let chest = Block::new("minecraft:chest", &[("facing", "south"), ("type", "left")]);
        assert_eq!(m.block(&chest).prop("type"), Some("right"));
    }

    #[test]
    fn axes_connections_and_rails_turn() {
        let t = Placement { rotation: 1, ..Default::default() };
        let log = Block::new("minecraft:oak_log", &[("axis", "x")]);
        assert_eq!(t.block(&log).prop("axis"), Some("z"));
        let fence = Block::new("minecraft:oak_fence", &[("east", "false"), ("north", "true"), ("south", "false"), ("west", "false")]);
        let f = t.block(&fence);
        assert_eq!(f.prop("east"), Some("true"));
        assert_eq!(f.prop("north"), Some("false"));
        let rail = Block::new("minecraft:rail", &[("shape", "north_east")]);
        assert_eq!(t.block(&rail).prop("shape"), Some("south_east"));
        let up = Block::new("minecraft:rail", &[("shape", "ascending_north")]);
        assert_eq!(t.block(&up).prop("shape"), Some("ascending_east"));
        let straight = Block::new("minecraft:powered_rail", &[("shape", "north_south")]);
        assert_eq!(t.block(&straight).prop("shape"), Some("east_west"));
        let sign = Block::new("minecraft:oak_sign", &[("rotation", "14")]);
        assert_eq!(t.block(&sign).prop("rotation"), Some("2"));
    }
}
