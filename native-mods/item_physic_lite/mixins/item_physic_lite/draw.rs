// `ItemEntityRenderer.submit` for every dropped item, as the game draws it, with
// Item Physic Lite's pose: no bobbing, no spinning, a flat item lying on its
// back, the others standing on the ground. Everything else is the game's own.
fn item_physic_lite_draw(
    game: &crate::game::Game<'_>,
    camera: glam::DVec3,
    alpha: f64,
    out: &mut Vec<Vertex>,
) -> core::ops::ControlFlow<()> {
    use crate::game::native_item_physic_lite as m;
    if !game.native_on(m::ID) {
        return core::ops::ControlFlow::Continue(());
    }
    let live = game.dropped.iter().map(|item| {
        let (p, q) = (item.previous, item.position);
        let at = [
            p.x + (q.x - p.x) * alpha,
            p.y + (q.y - p.y) * alpha,
            p.z + (q.z - p.z) * alpha,
        ];
        (item, at, alpha as f32, true)
    });
    // ItemPickupParticle: the render state extracted at partial tick 1.
    let taken = game.pickups.live.iter().filter_map(|p| match &p.taken {
        super::pickups::Taken::Item(item) => {
            Some((&**item, super::pickups::position(p, alpha), 1.0, false))
        }
        _ => None,
    });
    let range =
        super::entity_culling::default_range(glam::DVec3::splat(0.25), game.view_scale());
    let skinning = game.skinning();
    let lights = diffuse_lights(game.dim == Dim::Nether);
    for (item, at, partial, entity) in live.chain(taken) {
        let entity_start = out.len();
        game.mark_motion(entity_start, (2 << 32) | u64::from(item.id));
        let q = item.position;
        let pos = glam::DVec3::from_array(at) - camera;
        let far = if entity {
            (glam::DVec3::new(q.x, q.y, q.z) - camera).length_squared() >= range * range
        } else {
            pos.length_squared() > 64.0 * 64.0
        };
        if far {
            continue;
        }
        if entity && let Some(view) = &game.view_frustum {
            let feet = (glam::DVec3::new(q.x, q.y, q.z) - camera).as_vec3();
            let half = glam::Vec3::new(0.125 + 0.5, 0.5, 0.125 + 0.5);
            let lo = feet - half;
            let hi = feet + glam::Vec3::new(half.x, 0.25 + 0.5, half.z);
            if !view.intersects(lo, hi) {
                continue;
            }
        }
        // `extractItemGroupRenderState`: GROUND, seeded by the entity id.
        let owner = owner_at([q.x, q.y, q.z], 0.0, "minecraft:item");
        let state = game.resolve_item(
            &item.stack,
            DisplayContext::Ground,
            Some(&owner),
            item.id as i32,
        );
        if state.is_empty() {
            continue;
        }
        let (lo, hi) = state.bounding_box();
        let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
        let flat = m::is_flat(lo, hi);
        // The item keeps the way it was dropped facing (its bob offset is a
        // random angle); a flat one is turned onto its back, its face up.
        let mut pose = glam::Mat4::from_translation(pos.as_vec3())
            * glam::Mat4::from_translation(glam::Vec3::new(0.0, m::rest_height(flat, lo), 0.0))
            * glam::Mat4::from_rotation_y(item.bob_offset);
        if flat {
            pose *= glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        }
        let packed = game.world.light(
            q.x.floor() as i32,
            (q.y + 0.1).floor() as i32,
            q.z.floor() as i32,
        );
        let seed = i64::from(game.pack.item_renderer.item_id(&item.stack.item))
            + i64::from(item.stack.damage);
        let copies = item_render::cluster_poses(
            &state,
            pose,
            item_render::rendered_amount(item.stack.count),
            seed,
        );
        let glowing = game.entity_glowing(item.id);
        // Posed on the GPU when every layer can be, its silhouette too when it glows.
        let skinned = skinning
            .then(|| game.submit_item_skinned(&state, &copies, vertex_light(packed), lights))
            .flatten();
        if skinned.is_none() {
            for &copy in &copies {
                game.submit_item_world(&state, copy, vertex_light(packed), out);
            }
        }
        if glowing {
            let rgb = super::glowing::team_color(game.mob_team(item.id));
            game.mark_outline(&out[entity_start..], rgb);
            if let Some(((first, _, _), _)) = skinned {
                let color = horizon_client::mobs::render::abgr(rgb);
                game.skin_frame.borrow_mut().outline_since(first, color);
            }
        } else {
            game.cull_skinned(out, entity_start, skinned);
        }
    }
    core::ops::ControlFlow::Break(())
}
