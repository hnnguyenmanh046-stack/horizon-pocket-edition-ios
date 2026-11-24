// The overlay: the placement's box and the blocks still to fix, as outlines.
fn litematica_visuals(game: &crate::game::Game<'_>, lines: &mut Vec<horizon_client::gizmos::Line>) {
    crate::game::litematica::lines(game, lines);
}
