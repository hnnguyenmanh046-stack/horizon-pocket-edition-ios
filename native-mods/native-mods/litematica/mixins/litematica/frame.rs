// Each frame while the menu is open: its clicks applied, the latest view handed over.
fn litematica_frame(game: &mut crate::game::Game<'_>) {
    crate::game::litematica::frame(game);
}
