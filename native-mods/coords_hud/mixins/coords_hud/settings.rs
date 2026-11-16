// The mod's settings page: the box shown or hidden, its height on the screen
// (the number to copy to another phone), and the steps that move it.
fn official_coords_hud_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use crate::game::official_coords_hud as m;
    if mod_id != m::ID {
        return;
    }
    let shown = m::shown(|| game.native_setting(m::ID, "shown"));
    let y = m::y(|| game.native_setting(m::ID, "y"));
    buttons.extend(m::labels(shown, y));
}
