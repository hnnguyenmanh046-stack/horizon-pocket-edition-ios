// K opens the menu, Page Up / Page Down move the layer, Left Alt held for the wheel.
fn litematica_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::litematica::key(game, key, pressed);
}
