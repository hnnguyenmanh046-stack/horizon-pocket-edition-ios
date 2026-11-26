// Left Alt + wheel moves the placement the way you look.
fn litematica_wheel(game: &mut crate::game::Game<'_>, steps: f32, claimed: &mut bool) {
    crate::game::litematica::wheel(game, steps, claimed);
}
