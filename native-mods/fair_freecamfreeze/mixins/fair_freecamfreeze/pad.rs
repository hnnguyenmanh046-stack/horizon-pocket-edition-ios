// The touch stick and the pad move the player through `pad_move`, not through
// held keys. While the camera flies free it reads as centred, so the player
// stands still.
fn freecam_pad() -> core::ops::ControlFlow<Option<[f32; 2]>> {
    if crate::game::fair_freecam::active() {
        core::ops::ControlFlow::Break(None)
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
