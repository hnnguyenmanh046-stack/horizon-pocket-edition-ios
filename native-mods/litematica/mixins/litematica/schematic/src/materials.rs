//! What a block costs to place: the item (`minecraft:` left off) and how
//! many, or nothing (air, a door's top half, fire, portals...).

use crate::Block;

pub fn item_for(b: &Block) -> Option<(String, u32)> {
    let name = b.short();
    if b.is_air() {
        return None;
    }
    // The other half of a two-block thing is free.
    if b.prop("half") == Some("upper") || b.prop("part") == Some("head") {
        return None;
    }
    let count = match (b.prop("type"), name) {
        (Some("double"), n) if n.ends_with("_slab") => 2,
        _ => b
            .prop("candles")
            .or(b.prop("pickles"))
            .or(b.prop("eggs"))
            .or(b.prop("flower_amount"))
            .or(b.prop("segment_amount"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(1),
    };
    let item = match name {
        "piston_head" | "moving_piston" | "fire" | "soul_fire" | "nether_portal" | "end_portal" | "end_gateway"
        | "bubble_column" | "frosted_ice" | "light" | "barrier" | "structure_block" | "jigsaw" => return None,
        "water" => "water_bucket".into(),
        "lava" => "lava_bucket".into(),
        "powder_snow" => "powder_snow_bucket".into(),
        "redstone_wire" => "redstone".into(),
        "tripwire" => "string".into(),
        "wall_torch" => "torch".into(),
        "soul_wall_torch" => "soul_torch".into(),
        "redstone_wall_torch" => "redstone_torch".into(),
        "copper_wall_torch" => "copper_torch".into(),
        "carrots" => "carrot".into(),
        "potatoes" => "potato".into(),
        "beetroots" => "beetroot_seeds".into(),
        "wheat" => "wheat_seeds".into(),
        "melon_stem" | "attached_melon_stem" => "melon_seeds".into(),
        "pumpkin_stem" | "attached_pumpkin_stem" => "pumpkin_seeds".into(),
        "cocoa" => "cocoa_beans".into(),
        "sweet_berry_bush" => "sweet_berries".into(),
        "cave_vines" | "cave_vines_plant" => "glow_berries".into(),
        "kelp_plant" => "kelp".into(),
        "tall_seagrass" => "seagrass".into(),
        "twisting_vines_plant" => "twisting_vines".into(),
        "weeping_vines_plant" => "weeping_vines".into(),
        "big_dripleaf_stem" => "big_dripleaf".into(),
        "bamboo_sapling" => "bamboo".into(),
        "torchflower_crop" => "torchflower_seeds".into(),
        "pitcher_crop" => "pitcher_pod".into(),
        "dirt_path" | "farmland" => "dirt".into(),
        n if n.starts_with("potted_") => "flower_pot".into(),
        n if n.ends_with("_wall_hanging_sign") => n.replace("_wall_hanging_sign", "_hanging_sign"),
        n if n.ends_with("_wall_sign") => n.replace("_wall_sign", "_sign"),
        n if n.ends_with("_wall_banner") => n.replace("_wall_banner", "_banner"),
        n if n.ends_with("_wall_head") => n.replace("_wall_head", "_head"),
        n if n.ends_with("_wall_skull") => n.replace("_wall_skull", "_skull"),
        n if n.ends_with("_wall_fan") => n.replace("_wall_fan", "_fan"),
        n if n.ends_with("_candle_cake") || n == "candle_cake" => "cake".into(),
        n => n.into(),
    };
    Some((item, count))
}

/// An item id without `minecraft:`.
pub fn short_item(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_for_blocks() {
        let i = |n: &str, p: &[(&str, &str)]| item_for(&Block::new(n, p));
        assert_eq!(i("minecraft:stone", &[]), Some(("stone".into(), 1)));
        assert_eq!(i("minecraft:air", &[]), None);
        assert_eq!(i("minecraft:oak_door", &[("half", "upper")]), None);
        assert_eq!(i("minecraft:oak_door", &[("half", "lower")]), Some(("oak_door".into(), 1)));
        assert_eq!(i("minecraft:red_bed", &[("part", "head")]), None);
        assert_eq!(i("minecraft:stone_slab", &[("type", "double")]), Some(("stone_slab".into(), 2)));
        assert_eq!(i("minecraft:oak_wall_sign", &[]), Some(("oak_sign".into(), 1)));
        assert_eq!(i("minecraft:wall_torch", &[]), Some(("torch".into(), 1)));
        assert_eq!(i("minecraft:candle", &[("candles", "3")]), Some(("candle".into(), 3)));
        assert_eq!(i("minecraft:potted_poppy", &[]), Some(("flower_pot".into(), 1)));
        assert_eq!(i("minecraft:water", &[("level", "0")]), Some(("water_bucket".into(), 1)));
        assert_eq!(i("minecraft:piston_head", &[]), None);
        assert_eq!(short_item("minecraft:stone"), "stone");
    }
}
