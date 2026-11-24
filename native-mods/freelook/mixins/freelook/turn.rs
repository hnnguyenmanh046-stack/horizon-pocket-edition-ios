// A turn from anywhere (a finger, a pad), not only the mouse: while the key is held it
// turns the camera's angles, and the player's are put back after it.
fn freelook_turn(game: &mut crate::game::Game<'_>, xo: f64, yo: f64) -> std::ops::ControlFlow<()> {
    use crate::game::freelook as m;
    if !game.native_on(m::ID) || !game.native_window() || !m::begin() {
        return std::ops::ControlFlow::Continue(());
    }
    let (yaw, pitch) = (game.yaw, game.pitch);
    let (look_yaw, look_pitch) = m::look(yaw, pitch);
    game.yaw = look_yaw;
    game.pitch = look_pitch;
    game.turn_player(xo, yo);
    m::set_look(game.yaw, game.pitch);
    game.yaw = yaw;
    game.pitch = pitch;
    m::end();
    std::ops::ControlFlow::Break(())
}
