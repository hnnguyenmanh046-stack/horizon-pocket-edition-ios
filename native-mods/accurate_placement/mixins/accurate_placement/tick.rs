// Once a tick, before the tick's own clicks: place a block on each fresh spot
// the frames saw, then hold vanilla's own placing back for a tick.
fn abp_tick(game: &mut crate::game::Game<'_>) {
    use crate::game::accurate_placement as m;
    if !m::tick_gate() {
        return;
    }
    let Some(player) = game
        .player
        .as_ref()
        .map(|p| [p.position.x, p.position.y, p.position.z])
    else {
        return;
    };
    let held = game.held().cloned();
    if m::holding()
        && let Some(hit) = game.pick()
        && !game.remote_block_reacts(&hit, held.as_ref()).0.consumes_action()
    {
        game.use_delay = game.use_delay.max(1);
    }
    for t in m::due((game.yaw, game.pitch)) {
        if !m::still_fresh(&t, player) {
            continue;
        }
        let hit = crate::game::Hit {
            pos: t.pos,
            face: t.face,
            point: t.point,
        };
        if game.remote_block_reacts(&hit, held.as_ref()).0.consumes_action() {
            continue;
        }
        let at = t.placement();
        let before = game.world.block(at[0], at[1], at[2]);
        let kept = std::mem::replace(&mut game.tick_hit, Some(Some(hit)));
        game.remote_use();
        game.tick_hit = kept;
        if game.world.block(at[0], at[1], at[2]) != before {
            m::placed(&t, at, player);
        }
    }
}
