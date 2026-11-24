// Game::draw_debug_overlay HEAD: while F3 is open, SmartF3 draws its own debug screen
// in the vanilla one's place; otherwise (F3 closed, the mod off in Options > Mods, set
// to vanilla F3, or its file broken) the game's own runs. Horizon PE: the mod's own
// hooked `Game::hud` and rebuilt the whole HUD (native.rs `smartf3_overlay`).
// `HudBatch` is spelled as the target spells it.
fn smartf3_hud(
    game: &crate::game::Game<'_>,
    batch: &mut HudBatch,
    size: [u32; 2],
) -> core::ops::ControlFlow<()> {
    if game.smartf3_overlay(batch, size) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
