//! Litematica for Horizon: `.litematic` schematics (Litematica's own file
//! format, by masa), placed in the world, turned and mirrored, and their
//! materials. Independently written for Horizon; no Litematica code is used.
//!
//! A `.litematic` file is gzip-compressed NBT:
//! `Metadata` (Name, Author, Description, EnclosingSize, TotalBlocks...) and
//! `Regions`, each with a `Position` and a `Size` (either may be negative),
//! a `BlockStatePalette` (Name + Properties) and `BlockStates`, the palette
//! indices packed into longs with entries spanning long boundaries, at least
//! two bits each, ordered x, then z, then y.

pub mod materials;
pub mod nbt;
pub mod sponge;
pub mod transform;

use nbt::Tag;

/// A block state as named in the file: `minecraft:oak_stairs` with its
/// properties sorted by name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Block {
    pub name: String,
    pub props: Vec<(String, String)>,
}

impl Block {
    pub fn new(name: &str, props: &[(&str, &str)]) -> Self {
        let mut props: Vec<(String, String)> = props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        props.sort();
        Self { name: name.to_owned(), props }
    }
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(p) = self.props.iter_mut().find(|(k, _)| k == key) {
            p.1 = value.to_owned();
        }
    }
    /// The name without `minecraft:`.
    pub fn short(&self) -> &str {
        self.name.strip_prefix("minecraft:").unwrap_or(&self.name)
    }
    pub fn is_air(&self) -> bool {
        matches!(self.short(), "air" | "cave_air" | "void_air" | "structure_void")
    }
}

/// A region: its blocks in a box `size` big whose least corner is `min`
/// (relative to the schematic's origin).
#[derive(Clone, Debug)]
pub struct Region {
    pub name: String,
    pub min: [i32; 3],
    pub size: [i32; 3],
    pub palette: Vec<Block>,
    /// Palette indices, x fastest, then z, then y.
    pub blocks: Vec<u16>,
}

impl Region {
    pub fn volume(&self) -> usize {
        self.size.iter().map(|&s| s as usize).product()
    }
    /// The palette index at `(x, y, z)` inside the region.
    pub fn index_at(&self, x: i32, y: i32, z: i32) -> u16 {
        let [sx, _, sz] = self.size;
        self.blocks[(x + z * sx + y * sx * sz) as usize]
    }
    /// Every block that isn't air: its place relative to the schematic's
    /// origin and its palette index.
    pub fn solid(&self) -> impl Iterator<Item = ([i32; 3], u16)> + '_ {
        let [sx, _, sz] = self.size;
        let air: Vec<bool> = self.palette.iter().map(Block::is_air).collect();
        self.blocks.iter().enumerate().filter_map(move |(i, &b)| {
            if air.get(b as usize).copied().unwrap_or(true) {
                return None;
            }
            let i = i as i32;
            let (x, z, y) = (i % sx, (i / sx) % sz, i / (sx * sz));
            Some(([self.min[0] + x, self.min[1] + y, self.min[2] + z], b))
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Schematic {
    pub name: String,
    pub author: String,
    pub description: String,
    pub regions: Vec<Region>,
}

impl Schematic {
    /// The least and greatest corner of all its regions (relative to its origin).
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        let mut lo = [i32::MAX; 3];
        let mut hi = [i32::MIN; 3];
        for r in &self.regions {
            for k in 0..3 {
                lo[k] = lo[k].min(r.min[k]);
                hi[k] = hi[k].max(r.min[k] + r.size[k] - 1);
            }
        }
        if self.regions.is_empty() {
            return ([0; 3], [0; 3]);
        }
        (lo, hi)
    }
    pub fn size(&self) -> [i32; 3] {
        let (lo, hi) = self.bounds();
        [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1]
    }
    /// How many blocks aren't air.
    pub fn block_count(&self) -> usize {
        self.regions.iter().map(|r| r.solid().count()).sum()
    }
}

/// The most blocks a schematic may have (a 400 by 400 by 400 cube).
pub const MAX_VOLUME: usize = 64_000_000;

fn xyz(t: Option<&Tag>) -> Option<[i32; 3]> {
    let t = t?;
    Some([t.get("x")?.int()? as i32, t.get("y")?.int()? as i32, t.get("z")?.int()? as i32])
}

/// Entry `index` of `bits`-bit entries packed into `longs`, spanning longs
/// (`LitematicaBitArray`'s layout).
pub fn packed(longs: &[i64], bits: u32, index: usize) -> Option<u64> {
    let start_bit = index * bits as usize;
    let start = start_bit / 64;
    let end = (start_bit + bits as usize - 1) / 64;
    let offset = (start_bit % 64) as u32;
    let mask = if bits == 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let a = *longs.get(start)? as u64;
    if start == end {
        Some((a >> offset) & mask)
    } else {
        let b = *longs.get(end)? as u64;
        Some(((a >> offset) | (b << (64 - offset))) & mask)
    }
}

/// Bits per entry for a palette of `n`.
pub fn bits_for(n: usize) -> u32 {
    let mut bits = 2;
    while (1usize << bits) < n {
        bits += 1;
    }
    bits
}

/// Reads a .litematic file's bytes, or a .schem (Sponge/WorldEdit) file's.
pub fn load(bytes: &[u8]) -> Result<Schematic, String> {
    let root = nbt::read_gzip(bytes)?;
    if sponge::is_sponge(&root) {
        return sponge::load(&root);
    }
    let meta = root.get("Metadata");
    let text = |k: &str| meta.and_then(|m| m.get(k)).and_then(Tag::str).unwrap_or("").to_owned();
    let mut out = Schematic {
        name: text("Name"),
        author: text("Author"),
        description: text("Description"),
        regions: Vec::new(),
    };
    let regions = root.get("Regions").and_then(Tag::compound).ok_or("it has no regions (is it a .litematic or .schem file?)")?;
    let mut volume = 0usize;
    for (name, r) in regions {
        let pos = xyz(r.get("Position")).ok_or_else(|| format!("region {name} has no position"))?;
        let size = xyz(r.get("Size")).ok_or_else(|| format!("region {name} has no size"))?;
        if size.contains(&0) {
            continue;
        }
        // A negative size grows from the position towards the negative side.
        let min: [i32; 3] = std::array::from_fn(|k| if size[k] < 0 { pos[k] + size[k] + 1 } else { pos[k] });
        let size = size.map(i32::abs);
        let count = size.iter().map(|&s| s as usize).product::<usize>();
        volume = volume.saturating_add(count);
        if volume > MAX_VOLUME {
            return Err(format!("it's too big (more than {MAX_VOLUME} blocks)"));
        }
        let mut palette = Vec::new();
        for entry in r.get("BlockStatePalette").and_then(Tag::list).ok_or_else(|| format!("region {name} has no palette"))? {
            let block = entry.get("Name").and_then(Tag::str).unwrap_or("minecraft:air");
            let mut props: Vec<(String, String)> = entry
                .get("Properties")
                .and_then(Tag::compound)
                .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.str()?.to_owned()))).collect())
                .unwrap_or_default();
            props.sort();
            palette.push(Block { name: block.to_owned(), props });
        }
        if palette.is_empty() || palette.len() > u16::MAX as usize {
            return Err(format!("region {name} has a broken palette"));
        }
        let longs = r.get("BlockStates").and_then(Tag::longs).ok_or_else(|| format!("region {name} has no blocks"))?;
        let bits = bits_for(palette.len());
        if longs.len() * 64 < count * bits as usize {
            return Err(format!("region {name} has too few blocks for its size"));
        }
        let mut blocks = Vec::with_capacity(count);
        for i in 0..count {
            let v = packed(longs, bits, i).unwrap_or(0) as usize;
            blocks.push(if v < palette.len() { v as u16 } else { 0 });
        }
        out.regions.push(Region { name: name.clone(), min, size, palette, blocks });
    }
    if out.regions.is_empty() {
        return Err("it has no blocks".into());
    }
    Ok(out)
}

/// A schematic built in code, as a .litematic file's bytes (for the tests,
/// and for saving schematics later): one region at the origin.
pub fn save_one_region(name: &str, author: &str, size: [i32; 3], palette: &[Block], blocks: &[u16]) -> Vec<u8> {
    use std::collections::BTreeMap;
    let bits = bits_for(palette.len());
    let count = blocks.len();
    let mut longs = vec![0i64; (count * bits as usize).div_ceil(64)];
    for (i, &b) in blocks.iter().enumerate() {
        let start_bit = i * bits as usize;
        let (start, offset) = (start_bit / 64, start_bit % 64);
        let v = u64::from(b);
        longs[start] = (longs[start] as u64 | (v << offset)) as i64;
        if offset + bits as usize > 64 {
            longs[start + 1] = (longs[start + 1] as u64 | (v >> (64 - offset))) as i64;
        }
    }
    let v3 = |[x, y, z]: [i32; 3]| Tag::Compound(BTreeMap::from([("x".into(), Tag::Int(x)), ("y".into(), Tag::Int(y)), ("z".into(), Tag::Int(z))]));
    let pal = palette
        .iter()
        .map(|b| {
            let mut m = BTreeMap::from([("Name".to_owned(), Tag::String(b.name.clone()))]);
            if !b.props.is_empty() {
                m.insert("Properties".into(), Tag::Compound(b.props.iter().map(|(k, v)| (k.clone(), Tag::String(v.clone()))).collect()));
            }
            Tag::Compound(m)
        })
        .collect();
    let region = Tag::Compound(BTreeMap::from([
        ("Position".into(), v3([0, 0, 0])),
        ("Size".into(), v3(size)),
        ("BlockStatePalette".into(), Tag::List(pal)),
        ("BlockStates".into(), Tag::LongArray(longs)),
        ("TileEntities".into(), Tag::List(Vec::new())),
        ("Entities".into(), Tag::List(Vec::new())),
    ]));
    let meta = Tag::Compound(BTreeMap::from([
        ("Name".into(), Tag::String(name.into())),
        ("Author".into(), Tag::String(author.into())),
        ("Description".into(), Tag::String(String::new())),
        ("EnclosingSize".into(), v3(size)),
        ("RegionCount".into(), Tag::Int(1)),
    ]));
    let root = Tag::Compound(BTreeMap::from([
        ("Version".into(), Tag::Int(7)),
        ("MinecraftDataVersion".into(), Tag::Int(4554)),
        ("Metadata".into(), meta),
        ("Regions".into(), Tag::Compound(BTreeMap::from([(name.to_owned(), region)]))),
    ]));
    nbt::write::to_gzip(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_entries_span_longs() {
        // 5-bit entries: entry 12 starts at bit 60 and runs into the next long.
        let palette: Vec<Block> = (0..20).map(|i| Block::new(&format!("minecraft:b{i}"), &[])).collect();
        let blocks: Vec<u16> = (0..40).map(|i| (i * 7 % 20) as u16).collect();
        let bytes = save_one_region("t", "me", [40, 1, 1], &palette, &blocks);
        let s = load(&bytes).unwrap();
        assert_eq!(bits_for(20), 5);
        assert_eq!(s.regions[0].blocks, blocks);
    }

    #[test]
    fn small_palettes_use_two_bits() {
        assert_eq!(bits_for(1), 2);
        assert_eq!(bits_for(2), 2);
        assert_eq!(bits_for(4), 2);
        assert_eq!(bits_for(5), 3);
        assert_eq!(bits_for(256), 8);
        assert_eq!(bits_for(257), 9);
    }

    #[test]
    fn loads_a_house() {
        let palette = vec![
            Block::new("minecraft:air", &[]),
            Block::new("minecraft:stone", &[]),
            Block::new("minecraft:oak_stairs", &[("facing", "north"), ("half", "bottom"), ("shape", "straight"), ("waterlogged", "false")]),
        ];
        // 3 x 2 x 2: a floor of stone, one stair on top, the rest air.
        let mut blocks = vec![0u16; 12];
        for i in 0..6 {
            blocks[i] = 1;
        }
        blocks[6 + 1] = 2;
        let bytes = save_one_region("House", "Builder", [3, 2, 2], &palette, &blocks);
        let s = load(&bytes).unwrap();
        assert_eq!(s.name, "House");
        assert_eq!(s.author, "Builder");
        assert_eq!(s.size(), [3, 2, 2]);
        assert_eq!(s.block_count(), 7);
        let r = &s.regions[0];
        assert_eq!(r.palette[r.index_at(1, 1, 0) as usize].prop("facing"), Some("north"));
        let solid: Vec<_> = r.solid().collect();
        assert!(solid.contains(&([1, 1, 0], 2)));
        assert!(solid.contains(&([2, 0, 1], 1)));
    }

    #[test]
    fn negative_sizes_grow_backwards() {
        use std::collections::BTreeMap;
        let palette = [Block::new("minecraft:air", &[]), Block::new("minecraft:dirt", &[])];
        let bytes = save_one_region("n", "", [2, 1, 1], &palette, &[1, 1]);
        // Rewrite its Position/Size to (5,0,0) and (-2,1,1).
        let mut root = nbt::read_gzip(&bytes).unwrap();
        if let Tag::Compound(m) = &mut root
            && let Some(Tag::Compound(regions)) = m.get_mut("Regions")
            && let Some(Tag::Compound(r)) = regions.get_mut("n")
        {
            let v3 = |x, y, z| Tag::Compound(BTreeMap::from([("x".into(), Tag::Int(x)), ("y".into(), Tag::Int(y)), ("z".into(), Tag::Int(z))]));
            r.insert("Position".into(), v3(5, 0, 0));
            r.insert("Size".into(), v3(-2, 1, 1));
        }
        let s = load(&nbt::write::to_gzip(&root)).unwrap();
        assert_eq!(s.regions[0].min, [4, 0, 0]);
        assert_eq!(s.bounds(), ([4, 0, 0], [5, 0, 0]));
    }

    #[test]
    fn junk_is_refused_not_panicked_on() {
        assert!(load(b"not a schematic").is_err());
        assert!(load(&nbt::write::to_gzip(&Tag::Compound(Default::default()))).is_err());
        let mut truncated = save_one_region("x", "", [2, 2, 2], &[Block::new("minecraft:stone", &[])], &[0; 8]);
        truncated.truncate(truncated.len() / 2);
        assert!(load(&truncated).is_err());
    }
}
