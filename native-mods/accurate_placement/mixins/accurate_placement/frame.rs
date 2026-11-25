// Once a frame: sample the crosshair while right click is held with a block.
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
        && game.title.is_none()
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
}
