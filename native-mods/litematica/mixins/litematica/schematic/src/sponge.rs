//! `.schem` files (the Sponge schematic format that WorldEdit, FAWE and
//! others save), versions 1 to 3: gzip-compressed NBT with `Width`,
//! `Height` and `Length`, a `Palette` of block-state strings
//! (`minecraft:oak_stairs[facing=north,half=bottom]` -> index) and the
//! blocks as unsigned varints, x fastest, then z, then y. Version 3 keeps
//! them under `Blocks` (`Palette`, `Data`) inside a `Schematic` compound;
//! 1 and 2 have `Palette` and `BlockData` at the top. `Offset` is where the
//! box starts relative to where it was copied from, so a schematic placed
//! where you stand lands as WorldEdit would paste it.

use crate::nbt::Tag;
use crate::{Block, MAX_VOLUME, Region, Schematic};

/// Whether `root` looks like a Sponge schematic.
pub fn is_sponge(root: &Tag) -> bool {
    let s = root.get("Schematic").unwrap_or(root);
    s.get("Width").is_some() && (s.get("Palette").is_some() || s.get("Blocks").and_then(|b| b.get("Palette")).is_some())
}

/// `minecraft:oak_stairs[facing=north,half=bottom]` as a `Block`.
pub fn parse_state(text: &str) -> Block {
    let (name, rest) = match text.find('[') {
        Some(i) => (&text[..i], text[i + 1..].trim_end_matches(']')),
        None => (text, ""),
    };
    let mut props: Vec<(String, String)> = rest
        .split(',')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect();
    props.sort();
    let name = if name.contains(':') { name.trim().to_owned() } else { format!("minecraft:{}", name.trim()) };
    Block { name, props }
}

pub fn load(root: &Tag) -> Result<Schematic, String> {
    let s = root.get("Schematic").unwrap_or(root);
    let dim = |k: &str| s.get(k).and_then(Tag::int).map(|v| (v as i32) & 0xFFFF).ok_or_else(|| format!("it has no {k}"));
    let size = [dim("Width")?, dim("Height")?, dim("Length")?];
    if size.contains(&0) {
        return Err("it has no blocks".into());
    }
    let count = size.iter().map(|&v| v as usize).product::<usize>();
    if count > MAX_VOLUME {
        return Err(format!("it's too big (more than {MAX_VOLUME} blocks)"));
    }
    let blocks_tag = s.get("Blocks");
    let palette_tag = blocks_tag.and_then(|b| b.get("Palette")).or(s.get("Palette")).and_then(Tag::compound).ok_or("it has no palette")?;
    let data = match blocks_tag.and_then(|b| b.get("Data")).or(s.get("BlockData")) {
        Some(Tag::ByteArray(d)) => d,
        _ => return Err("it has no block data".into()),
    };
    // The palette by index; gaps stay air.
    let most = palette_tag.values().filter_map(Tag::int).max().unwrap_or(-1);
    if most < 0 || most >= u16::MAX as i64 {
        return Err("it has a broken palette".into());
    }
    let mut palette = vec![Block::new("minecraft:air", &[]); most as usize + 1];
    for (text, i) in palette_tag {
        if let Some(i) = i.int().filter(|&i| i >= 0) {
            palette[i as usize] = parse_state(text);
        }
    }
    let mut blocks = Vec::with_capacity(count);
    let mut at = 0usize;
    while blocks.len() < count {
        let mut value = 0u32;
        let mut shift = 0;
        loop {
            let Some(&b) = data.get(at) else {
                return Err("its block data ends early".into());
            };
            at += 1;
            value |= u32::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err("its block data is broken".into());
            }
        }
        blocks.push(if (value as usize) < palette.len() { value as u16 } else { 0 });
    }
    let offset = match s.get("Offset") {
        Some(Tag::IntArray(o)) if o.len() == 3 => [o[0], o[1], o[2]],
        _ => [0, 0, 0],
    };
    let meta = s.get("Metadata");
    let text = |k: &str| meta.and_then(|m| m.get(k)).and_then(Tag::str).unwrap_or("").to_owned();
    Ok(Schematic {
        name: text("Name"),
        author: text("Author"),
        description: String::new(),
        regions: vec![Region { name: "Main".into(), min: offset, size, palette, blocks }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn compound(items: Vec<(&str, Tag)>) -> Tag {
        Tag::Compound(items.into_iter().map(|(k, v)| (k.to_owned(), v)).collect::<BTreeMap<_, _>>())
    }

    #[test]
    fn states_parse() {
        let b = parse_state("minecraft:oak_stairs[half=bottom,facing=north]");
        assert_eq!(b.name, "minecraft:oak_stairs");
        assert_eq!(b.props, vec![("facing".into(), "north".into()), ("half".into(), "bottom".into())]);
        assert_eq!(parse_state("stone"), Block::new("minecraft:stone", &[]));
    }

    #[test]
    fn version_3_loads() {
        // 2 x 1 x 2: stone, air / air, a stair; one index over 127 to need two varint bytes.
        let mut pal = vec![
            ("minecraft:air", Tag::Int(0)),
            ("minecraft:stone", Tag::Int(1)),
            ("minecraft:oak_stairs[facing=east,half=bottom]", Tag::Int(200)),
        ];
        let palette = compound(pal.drain(..).collect());
        let data = vec![1u8, 0, 0, 0xC8, 0x01];
        let root = compound(vec![(
            "Schematic",
            compound(vec![
                ("Version", Tag::Int(3)),
                ("Width", Tag::Short(2)),
                ("Height", Tag::Short(1)),
                ("Length", Tag::Short(2)),
                ("Offset", Tag::IntArray(vec![-1, 0, 3])),
                ("Blocks", compound(vec![("Palette", palette), ("Data", Tag::ByteArray(data))])),
            ]),
        )]);
        let bytes = crate::nbt::write::to_gzip(&root);
        let s = crate::load(&bytes).unwrap();
        let r = &s.regions[0];
        assert_eq!(r.min, [-1, 0, 3]);
        assert_eq!(r.size, [2, 1, 2]);
        assert_eq!(r.palette[r.blocks[0] as usize].short(), "stone");
        assert_eq!(r.palette[r.blocks[3] as usize].prop("facing"), Some("east"));
        assert_eq!(s.block_count(), 2);
    }

    #[test]
    fn version_2_loads_and_short_data_is_refused() {
        let palette = compound(vec![("minecraft:air", Tag::Int(0)), ("minecraft:dirt", Tag::Int(1))]);
        let make = |data: Vec<u8>| {
            compound(vec![
                ("Version", Tag::Int(2)),
                ("Width", Tag::Short(1)),
                ("Height", Tag::Short(2)),
                ("Length", Tag::Short(1)),
                ("Palette", palette.clone()),
                ("BlockData", Tag::ByteArray(data)),
            ])
        };
        let s = crate::load(&crate::nbt::write::to_gzip(&make(vec![1, 1]))).unwrap();
        assert_eq!(s.block_count(), 2);
        assert!(crate::load(&crate::nbt::write::to_gzip(&make(vec![1]))).is_err());
    }

    /// Set LM_SCHEM to a real .schem file's path to check it loads.
    #[test]
    fn a_real_file_loads() {
        let Ok(path) = std::env::var("LM_SCHEM") else { return };
        let s = crate::load(&std::fs::read(&path).unwrap()).unwrap();
        let r = &s.regions[0];
        eprintln!("{path}: size {:?} offset {:?}, {} palette entries, {} blocks", r.size, r.min, r.palette.len(), s.block_count());
        let mut counts = std::collections::BTreeMap::new();
        for (_, i) in r.solid() {
            *counts.entry(r.palette[i as usize].short().to_owned()).or_insert(0) += 1;
        }
        eprintln!("{counts:?}");
    }
}
