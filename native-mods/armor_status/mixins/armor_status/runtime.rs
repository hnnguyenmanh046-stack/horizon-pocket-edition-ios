// ArmorStatusHUD native mod for Horizon: displays equipped armor pieces and
// held items with their true 1:1 Java durability color coding (Green > 50%,
// Yellow 25-50%, Orange 10-25%, Red < 10%) and configurable positioning.
// Horizon PE: the display's size is a setting (50% to 150%), and it starts left
// of the hotbar without its box.
// This file is both the standalone Cargo library (for unit tests) and the
// runtime module injected into `crates/horizon-client/src/game.rs`.
use std::cell::RefCell;

pub const ID: &str = "armor_status";
pub const NAME: &str = "Armor Status";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str =
    "Displays equipped armor and held items with precise durability and 1:1 Java original color coding (Green/Yellow/Orange/Red). Its size is a setting.";

/// Original Java ArmorStatusHUD color constants.
pub const COLOR_GREEN: u32 = 0x55_FF55;   // > 50% durability
pub const COLOR_YELLOW: u32 = 0xFF_FF55;  // 25% - 50% durability
pub const COLOR_ORANGE: u32 = 0xFF_AA00;  // 10% - 25% durability
pub const COLOR_RED: u32 = 0xFF_5555;     // < 10% durability
pub const COLOR_COUNT: u32 = 0xFF_FFFF;   // Non-damageable item stack count
pub const COLOR_DIM: u32 = 0xAA_AAAA;     // Empty / placeholder
pub const BACKGROUND: u32 = 0x7800_0000;  // Translucent dark box

pub const POSITIONS: [&str; 6] = [
    "Hotbar left",
    "Hotbar right",
    "Top left",
    "Bottom left",
    "Top right",
    "Bottom right",
];

pub const FORMATS: [&str; 3] = [
    "Remaining (1520)",
    "Percent (97%)",
    "Fraction (1520/1561)",
];

/// The display's sizes, in percent of its own: the icons and their numbers
/// together.
pub const SIZES: [u32; 10] = [50, 60, 70, 75, 80, 90, 100, 110, 125, 150];
/// `SIZES`' 100%.
pub const SIZE_FULL: usize = 6;

pub const HANDS: [&str; 4] = [
    "Main & off hand",
    "Main hand only",
    "Off hand only",
    "Off (Armor only)",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Index in `POSITIONS`. Default: 0 (Hotbar left).
    pub position: usize,
    /// Index in `FORMATS`. Default: 0 (Remaining value).
    pub format: usize,
    /// Index in `HANDS`. Default: 0 (Both hands).
    pub hands: usize,
    /// Draw a translucent dark background box behind items.
    pub background: bool,
    /// Show stack count for items without durability (e.g. 64 torches).
    pub show_count: bool,
    /// Index in `SIZES`. Default: `SIZE_FULL` (100%).
    pub size: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: 0, // Default: Hotbar left
            format: 0,
            hands: 0,
            background: false,
            show_count: true,
            size: SIZE_FULL,
        }
    }
}

impl Settings {
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let position = get("position")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .map_or(0, |p| p % POSITIONS.len());
        let format = get("format")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .map_or(0, |f| f % FORMATS.len());
        let hands = get("hands")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .map_or(0, |h| h % HANDS.len());
        let flag = |key: &str, def: bool| {
            get(key).map_or(def, |v| v.trim() != "false")
        };
        // Kept as the percent itself: the nearest size there is.
        let size = get("size")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .map_or(SIZE_FULL, |percent| {
                (0..SIZES.len())
                    .min_by_key(|i| SIZES[*i].abs_diff(percent))
                    .unwrap_or(SIZE_FULL)
            });
        Self {
            position,
            format,
            hands,
            background: flag("background", false),
            show_count: flag("show_count", true),
            size,
        }
    }

    /// The display's scale: 1.0 at 100%.
    pub fn scale(&self) -> f32 {
        SIZES[self.size % SIZES.len()] as f32 / 100.0
    }

    pub fn labels(&self) -> Vec<String> {
        let on = |b: bool| if b { "ON" } else { "OFF" };
        vec![
            format!("Position: {}", POSITIONS[self.position]),
            format!("Durability: {}", FORMATS[self.format]),
            format!("Hand items: {}", HANDS[self.hands]),
            format!("Box background: {}", on(self.background)),
            format!("Show stack count: {}", on(self.show_count)),
            format!("Size: {}%", SIZES[self.size % SIZES.len()]),
        ]
    }

    pub fn click(&mut self, i: usize) {
        match i {
            0 => self.position = (self.position + 1) % POSITIONS.len(),
            1 => self.format = (self.format + 1) % FORMATS.len(),
            2 => self.hands = (self.hands + 1) % HANDS.len(),
            3 => self.background = !self.background,
            4 => self.show_count = !self.show_count,
            5 => self.size = (self.size + 1) % SIZES.len(),
            _ => {}
        }
    }

    pub fn stored(&self) -> [(&'static str, String); 6] {
        let flag = |b: bool| if b { "true" } else { "false" }.to_owned();
        [
            ("position", self.position.to_string()),
            ("format", self.format.to_string()),
            ("hands", self.hands.to_string()),
            ("background", flag(self.background)),
            ("show_count", flag(self.show_count)),
            ("size", SIZES[self.size % SIZES.len()].to_string()),
        ]
    }
}

thread_local! {
    static SETTINGS: RefCell<Option<Settings>> = const { RefCell::new(None) };
}

pub fn settings(get: impl Fn(&str) -> Option<String>) -> Settings {
    SETTINGS.with_borrow_mut(|s| *s.get_or_insert_with(|| Settings::read(get)))
}

pub fn set_settings(new_settings: Settings) {
    SETTINGS.with_borrow_mut(|s| *s = Some(new_settings));
}

/// Calculates the exact 1:1 Java ArmorStatusHUD color for a given durability ratio.
pub fn durability_color(ratio: f32) -> u32 {
    if !ratio.is_finite() || ratio < 0.10 {
        COLOR_RED
    } else if ratio < 0.25 {
        COLOR_ORANGE
    } else if ratio <= 0.50 {
        COLOR_YELLOW
    } else {
        COLOR_GREEN
    }
}

/// Formats the durability text according to the selected format index.
pub fn format_durability(remaining: u32, max_damage: u32, format: usize) -> String {
    match format {
        1 => {
            let pct = if max_damage > 0 {
                ((remaining as f32 / max_damage as f32) * 100.0).round() as u32
            } else {
                100
            };
            format!("{pct}%")
        }
        2 => format!("{remaining}/{max_damage}"),
        _ => remaining.to_string(),
    }
}

/// Evaluates an item stack's display text and color for ArmorStatusHUD.
/// Returns `Some((text, color))` if text should be displayed beside the icon.
pub fn format_item(
    count: u32,
    damage: u32,
    max_damage: u32,
    format: usize,
    show_count: bool,
) -> Option<(String, u32)> {
    if max_damage > 0 {
        let remaining = max_damage.saturating_sub(damage);
        let ratio = remaining as f32 / max_damage as f32;
        let color = durability_color(ratio);
        let text = format_durability(remaining, max_damage, format);
        Some((text, color))
    } else if show_count && count > 1 {
        Some((count.to_string(), COLOR_COUNT))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |key| pairs.iter().find(|p| p.0 == key).map(|p| p.1.to_owned())
    }

    #[test]
    fn reads_and_writes_settings_correctly() {
        let defaults = Settings::read(stored(&[]));
        assert_eq!(defaults, Settings::default());
        assert_eq!(defaults.position, 0); // Hotbar left
        assert!(!defaults.background);
        assert_eq!(defaults.scale(), 1.0);

        let custom = Settings::read(stored(&[
            ("position", "1"),
            ("format", "2"),
            ("hands", "3"),
            ("background", "true"),
            ("show_count", "false"),
            ("size", "75"),
        ]));
        assert_eq!(
            custom,
            Settings {
                position: 1,
                format: 2,
                hands: 3,
                background: true,
                show_count: false,
                size: 3,
            }
        );
        assert_eq!(custom.scale(), 0.75);
        // A percent that isn't one of the sizes: the nearest.
        assert_eq!(Settings::read(stored(&[("size", "72")])).size, 2);

        let pairs = custom.stored();
        let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(Settings::read(stored(&refs)), custom);
    }

    #[test]
    fn button_clicks_cycle_options() {
        let mut s = Settings::default();
        assert_eq!(s.position, 0); // Default is Hotbar left
        s.click(0); // position cycles -> Hotbar right (1)
        assert_eq!(s.position, 1);
        s.click(1); // format -> Percent
        assert_eq!(s.format, 1);
        s.click(2); // hands -> Main hand only
        assert_eq!(s.hands, 1);
        s.click(3); // background -> true
        assert!(s.background);
        s.click(5); // size -> 110%
        assert_eq!(s.labels()[5], "Size: 110%");
        for _ in 0..3 {
            s.click(5);
        }
        assert_eq!(s.labels()[5], "Size: 50%"); // past 150%, the smallest again
        s.click(4); // show_count -> false
        assert!(!s.show_count);
    }

    #[test]
    fn original_java_color_thresholds_match_parity() {
        // > 50%: Green
        assert_eq!(durability_color(1.0), COLOR_GREEN);
        assert_eq!(durability_color(0.51), COLOR_GREEN);

        // 25% - 50%: Yellow
        assert_eq!(durability_color(0.50), COLOR_YELLOW);
        assert_eq!(durability_color(0.25), COLOR_YELLOW);

        // 10% - 25%: Orange
        assert_eq!(durability_color(0.24), COLOR_ORANGE);
        assert_eq!(durability_color(0.10), COLOR_ORANGE);

        // < 10%: Red
        assert_eq!(durability_color(0.09), COLOR_RED);
        assert_eq!(durability_color(0.01), COLOR_RED);
        assert_eq!(durability_color(0.00), COLOR_RED);
    }

    #[test]
    fn formats_durability_styles() {
        assert_eq!(format_durability(1500, 1561, 0), "1500");
        assert_eq!(format_durability(1500, 1561, 1), "96%");
        assert_eq!(format_durability(1500, 1561, 2), "1500/1561");
    }

    #[test]
    fn item_formatting_handles_durability_and_stacks() {
        // Diamond sword at 1500/1561 damage
        let sword = format_item(1, 61, 1561, 0, true);
        assert_eq!(sword, Some(("1500".into(), COLOR_GREEN)));

        // Critical diamond sword at 50/1561 damage
        let low_sword = format_item(1, 1511, 1561, 0, true);
        assert_eq!(low_sword, Some(("50".into(), COLOR_RED)));

        // Non-damageable stack (64 golden apples)
        let stack = format_item(64, 0, 0, 0, true);
        assert_eq!(stack, Some(("64".into(), COLOR_COUNT)));

        // Single item without damage -> no count displayed
        let single = format_item(1, 0, 0, 0, true);
        assert_eq!(single, None);
    }
}
