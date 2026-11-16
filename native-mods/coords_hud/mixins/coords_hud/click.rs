// A click on the mod's settings page: show or hide the box, set its height
// (automatic, or the player's own) or move it by a step, and keep it.
fn official_coords_hud_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use crate::game::official_coords_hud as m;
    if mod_id != m::ID {
        return;
    }
    if button == 0 {
        let shown = !m::shown(|| game.native_setting(m::ID, "shown"));
        m::set_shown(shown);
        game.native_set_setting(m::ID, "shown", if shown { "true" } else { "false" });
        return;
    }
    let y = m::y(|| game.native_setting(m::ID, "y"));
    let (drawn, most) = m::drawn();
    let y = match button {
        // The player's own height starts where the box is; automatic again from it.
        1 if y.is_some() => None,
        1 => Some(drawn.clamp(0, most.max(0))),
        _ => match m::STEPS.get(button - 2) {
            Some(step) => Some(m::stepped(y, drawn, *step, most)),
            None => return,
        },
    };
    m::set_y(y);
    game.native_set_setting(m::ID, "y", &m::stored_y(y));
}
