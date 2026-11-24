// Horizon PE (0.9.0 engine): `server_started`, which fires where `world_lifecycle`'s `WorldOpenReady` did.
fn jade_rule_world_open(game: &mut crate::game::Game<'_>) {
    crate::game::jade::rule_world_open(game);
}
