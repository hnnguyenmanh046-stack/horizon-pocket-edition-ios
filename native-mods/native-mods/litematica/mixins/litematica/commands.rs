// /litematica and /lm join the chat's commands.
fn litematica_commands(commands: &mut Vec<&'static str>) {
    crate::game::litematica::commands(commands);
}
