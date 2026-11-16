// native_hooks::Game::native_tooltip HEAD (a hook point): every item tooltip
// the game shows comes through here with its lines built; an enchanted
// stack's gets a hint under each enchantment, anything else is left as it is.
fn enchantmenthints_tooltip(
    game: &crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    lines: &mut Vec<(horizon_client::text::Text, u32)>,
) {
    game.enchantmenthints_tooltip(stack, lines);
}
