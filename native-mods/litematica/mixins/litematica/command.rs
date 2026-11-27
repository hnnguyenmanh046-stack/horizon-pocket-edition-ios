// /litematica (or /lm) list, load, here, move, rotate, mirror, overlay, layer, unload.
fn litematica_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    crate::game::litematica::command(game, command, ran);
}
