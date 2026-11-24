// Each tick: the last placement put back, and the blocks checked.
fn litematica_tick(game: &mut crate::game::Game<'_>) {
    crate::game::litematica::tick(game);
}
