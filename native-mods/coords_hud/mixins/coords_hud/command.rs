// /where puts the position in chat; /coords hides or shows the box, and
// /coords y <number> (or /coords y auto) sets its height on the screen.
fn coords_hud_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use crate::game::official_coords_hud as m;
    let mut words = command.split(' ').filter(|w| !w.is_empty());
    let name = words.next().unwrap_or("");
    if !game.native_on(m::ID) || !matches!(name, "where" | "coords") {
        return;
    }
    *ran = true;
    if name == "coords" {
        if words.next() == Some("y") {
            match m::command_y(words.next().unwrap_or("")) {
                Some(y) => {
                    // Kept as asked; the box itself never leaves the screen.
                    m::set_y(y);
                    game.native_set_setting(m::ID, "y", &m::stored_y(y));
                    game.native_chat(&match y {
                        Some(y) => format!("Coordinates panel: {y} from the top"),
                        None => "Coordinates panel: automatic height".to_owned(),
                    });
                }
                None => game.native_chat("Usage: /coords y <number>, or /coords y auto"),
            }
            return;
        }
        let shown = !m::shown(|| game.native_setting(m::ID, "shown"));
        m::set_shown(shown);
        game.native_set_setting(m::ID, "shown", if shown { "true" } else { "false" });
        game.native_chat(if shown { "Coordinates panel shown" } else { "Coordinates panel hidden" });
        return;
    }
    match game.native_player() {
        Some(p) => game.native_chat(&m::where_line(p.x, p.y, p.z, p.dimension)),
        None => game.native_chat("Not in a world"),
    }
}
