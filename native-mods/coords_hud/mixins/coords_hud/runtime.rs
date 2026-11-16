// Horizon's Coordinates HUD, formerly the WebAssembly SDK's example mod: the
// block position and the direction the player faces in a box in the top-left
// corner, `/where` to put the position in chat and `/coords` to hide or show
// the box. This file is both the mod's Cargo library (its tests) and the
// module the bundle adds to the game.
//
// Horizon PE: the box can be put at any height on the left edge (its settings
// page, or `/coords y <number>`), so that nothing covers it; the page says the
// height it is at, to set the same on another phone.

/// The mod's id (its settings folder), name and version.
pub const ID: &str = "coords_hud";
pub const NAME: &str = "Coordinates HUD";
pub const VERSION: &str = "2.0.0";
pub const DESCRIPTION: &str = "Your coordinates and facing in a corner of the screen. /where puts them in chat, /coords hides or shows the box. Its height on the screen is set on its settings page, or with /coords y <number>.";
/// The WebAssembly mods' panel: translucent black, white text with a shadow.
pub const BACKGROUND: u32 = 0x7800_0000;
pub const INK: u32 = 0xFF_FFFF;
/// The box's distance from the left edge, as the corner's boxes have.
pub const LEFT: f32 = 2.0;
/// The settings page's buttons after `Shown`: the height, then the steps it moves by.
pub const STEPS: [i32; 4] = [-1, 1, -10, 10];

/// Direction.fromYRot: yaw 0 faces south, 90 west.
pub fn facing(yaw: f32) -> &'static str {
    let i = (yaw / 90.0 + 0.5).floor() as i32 & 3;
    ["South (+Z)", "West (-X)", "North (-Z)", "East (+X)"][i as usize]
}

/// The box's lines, as the WebAssembly mod wrote them.
pub fn lines(x: f64, y: f64, z: f64, yaw: f32) -> [String; 2] {
    [
        format!("XYZ {x:.1} / {y:.1} / {z:.1}"),
        format!("Facing {}", facing(yaw)),
    ]
}

/// `/where`'s line: `12, 70, -4 in minecraft:the_nether`.
pub fn where_line(x: f64, y: f64, z: f64, dimension: &str) -> String {
    format!(
        "{:.0}, {:.0}, {:.0} in {dimension}",
        x.floor(),
        y.floor(),
        z.floor()
    )
}

/// The stored `shown` value: shown unless it says `false`.
pub fn read_shown(stored: Option<String>) -> bool {
    stored.is_none_or(|v| v.trim() != "false")
}

pub fn shown_label(shown: bool) -> String {
    format!("Shown: {}", if shown { "ON" } else { "OFF" })
}

/// The stored `y` value: the box's height from the top of the screen in GUI
/// pixels; none (missing, `auto`, or not a number) is the corner's own place.
pub fn read_y(stored: Option<String>) -> Option<i32> {
    stored
        .and_then(|v| v.trim().parse::<i32>().ok())
        .map(|y| y.max(0))
}

/// What to store for a height.
pub fn stored_y(y: Option<i32>) -> String {
    y.map_or_else(|| "auto".to_owned(), |y| y.to_string())
}

/// The height's button: where the box is, in words a player can copy to
/// another phone.
pub fn y_label(y: Option<i32>) -> String {
    match y {
        Some(y) => format!("Height: {y} from the top (tap for automatic)"),
        None => "Height: Automatic (tap to set your own)".to_owned(),
    }
}

/// A step's button.
pub fn step_label(step: i32) -> String {
    let way = if step < 0 { "up" } else { "down" };
    format!("Move {way} {}", step.abs())
}

/// The settings page's buttons.
pub fn labels(shown: bool, y: Option<i32>) -> Vec<String> {
    let mut labels = vec![shown_label(shown), y_label(y)];
    labels.extend(STEPS.iter().map(|step| step_label(*step)));
    labels
}

/// The height after a step from `y` (or from where the box was last drawn,
/// when it was automatic), kept on a screen `most` high for the box.
pub fn stepped(y: Option<i32>, drawn: i32, step: i32, most: i32) -> i32 {
    (y.unwrap_or(drawn).min(most) + step).clamp(0, most.max(0))
}

thread_local! {
    /// Whether the box shows, read once from its file, then kept.
    static SHOWN: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    /// The box's height, read once from its file, then kept (none inside: automatic).
    static Y: std::cell::Cell<Option<Option<i32>>> = const { std::cell::Cell::new(None) };
    /// Where the box was last drawn, and the lowest it can be on this screen.
    static DRAWN: std::cell::Cell<(i32, i32)> = const { std::cell::Cell::new((2, 10_000)) };
}

/// Whether the box shows, read once through `get` (the `shown` file).
pub fn shown(get: impl FnOnce() -> Option<String>) -> bool {
    SHOWN.with(|s| {
        let found = s.get().unwrap_or_else(|| read_shown(get()));
        s.set(Some(found));
        found
    })
}

/// Replaces it (`/coords` or the settings page changed it).
pub fn set_shown(shown: bool) {
    SHOWN.with(|s| s.set(Some(shown)));
}

/// The box's height, read once through `get` (the `y` file).
pub fn y(get: impl FnOnce() -> Option<String>) -> Option<i32> {
    Y.with(|s| {
        let found = s.get().unwrap_or_else(|| read_y(get()));
        s.set(Some(found));
        found
    })
}

/// Replaces it (`/coords y` or the settings page changed it).
pub fn set_y(y: Option<i32>) {
    Y.with(|s| s.set(Some(y)));
}

/// The HUD says where it drew the box, and the lowest the box can go.
pub fn drew(y: i32, most: i32) {
    DRAWN.with(|d| d.set((y, most)));
}

/// Where the box was last drawn, and the lowest it can go on this screen.
pub fn drawn() -> (i32, i32) {
    DRAWN.with(std::cell::Cell::get)
}

/// `/coords y ...`'s argument: a height, or `auto`. None for anything else.
pub fn command_y(argument: &str) -> Option<Option<i32>> {
    let argument = argument.trim();
    if argument.eq_ignore_ascii_case("auto") {
        return Some(None);
    }
    argument.parse::<i32>().ok().map(|y| Some(y.max(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_follows_vanilla_yaw() {
        assert_eq!(facing(0.0), "South (+Z)");
        assert_eq!(facing(90.0), "West (-X)");
        assert_eq!(facing(-180.0), "North (-Z)");
        assert_eq!(facing(-90.0), "East (+X)");
        assert_eq!(facing(314.0), "East (+X)");
    }

    #[test]
    fn lines_read_as_the_webassembly_mods() {
        assert_eq!(
            lines(12.5, 64.0, -3.25, 180.0),
            ["XYZ 12.5 / 64.0 / -3.2", "Facing North (-Z)"]
        );
        assert_eq!(
            where_line(12.5, 70.2, -3.25, "minecraft:the_nether"),
            "12, 70, -4 in minecraft:the_nether"
        );
    }

    #[test]
    fn shown_unless_stored_false() {
        assert!(read_shown(None));
        assert!(read_shown(Some("true".into())));
        assert!(!read_shown(Some("false\n".into())));
        assert_eq!(shown_label(false), "Shown: OFF");
    }

    #[test]
    fn the_height_is_automatic_until_set_and_says_where_it_is() {
        assert_eq!(read_y(None), None);
        assert_eq!(read_y(Some("auto".into())), None);
        assert_eq!(read_y(Some("37\n".into())), Some(37));
        assert_eq!(read_y(Some("-4".into())), Some(0));
        assert_eq!(stored_y(None), "auto");
        assert_eq!(stored_y(Some(37)), "37");
        assert_eq!(read_y(Some(stored_y(Some(120)))), Some(120));
        assert_eq!(y_label(Some(37)), "Height: 37 from the top (tap for automatic)");
        assert_eq!(
            labels(true, None),
            [
                "Shown: ON",
                "Height: Automatic (tap to set your own)",
                "Move up 1",
                "Move down 1",
                "Move up 10",
                "Move down 10",
            ]
        );
    }

    #[test]
    fn steps_start_where_the_box_is_and_stay_on_the_screen() {
        // Automatic, drawn at 2: a step down starts from there.
        assert_eq!(stepped(None, 2, 10, 200), 12);
        assert_eq!(stepped(Some(12), 2, -1, 200), 11);
        assert_eq!(stepped(Some(3), 2, -10, 200), 0);
        assert_eq!(stepped(Some(195), 2, 10, 200), 200);
        // A height from a taller screen comes back onto this one.
        assert_eq!(stepped(Some(400), 2, -1, 200), 199);
        assert_eq!(command_y("40"), Some(Some(40)));
        assert_eq!(command_y(" AUTO "), Some(None));
        assert_eq!(command_y("-3"), Some(Some(0)));
        assert_eq!(command_y("high"), None);
    }
}
