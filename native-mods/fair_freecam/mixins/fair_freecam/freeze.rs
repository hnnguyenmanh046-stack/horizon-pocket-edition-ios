// The player's simulation tick: skipped entirely while the camera flies free,
// so no gravity, no momentum and no stick or pad input move the body.
fn fair_freecam_freeze() -> core::ops::ControlFlow<Result<()>> {
    if crate::motion::fair_freecam_gate::frozen() {
        core::ops::ControlFlow::Break(Ok(()))
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
