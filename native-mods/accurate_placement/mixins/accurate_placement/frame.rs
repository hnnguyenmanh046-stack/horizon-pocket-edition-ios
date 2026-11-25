fn abp_frame(game: &mut crate::game::Game<'_>) {
    use crate::game::accurate_placement as m;
    let settings = m::Settings::default();
    let Some(player) = game
        .player
        .as_ref()
        .map(|p| [p.position.x, p.position.y, p.position.z])
    else {
        m::reset();
        return;
    };
    // Only in your own worlds: on a remote server the game's own placing rate applies.
    let allowed = settings.enabled && game.integrated.is_some();
    let held = match game.held() {
        Some(stack) => Some(stack.clone()),
        None => game.offhand.clone(),
    };
    let block_item = held
        .as_ref()
        .and_then(|s| game.items().get(&s.item))
        .is_some_and(|i| i.block.is_some());
    let mut down = allowed
        && block_item
        && game.right
        && game.screen == crate::game::Screen::Playing
        && game.sleeping.is_none()
        && game.using.is_none();
    let mut target = None;
    if down && !game.entity_targeted() {
        if let Some(hit) = game.pick() {
            let state = game.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
            let scaffolding = held
                .as_ref()
                .is_some_and(|s| &*s.item == "minecraft:scaffolding")
                && &*game.res.facts().get(state).name == "minecraft:scaffolding";
            if scaffolding {
                down = false;
            } else {
                target = Some(m::Sample {
                    pos: hit.pos,
                    face: hit.face,
                    point: hit.point,
                    replaceable: game.res.facts().get(state).replaceable,
                });
            }
        }
    }
    m::frame(down, target, player, (game.yaw, game.pitch));
    if !m::tick_gate() {
        return;
    }
    {
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
            inside: false,
        };
        if game.remote_block_reacts(&hit, held.as_ref()).0.consumes_action() {
            continue;
        }
        let at = t.placement();
        let before = game.world.block(at[0], at[1], at[2]);
        let kept = game.tick_hit.replace(Some(Some(hit)));
        game.remote_use(mod_api::InteractionAttempt::Press);
        game.tick_hit.set(kept);
        if game.world.block(at[0], at[1], at[2]) != before {
            m::placed(&t, at, player);
        }
    }
    }
}

