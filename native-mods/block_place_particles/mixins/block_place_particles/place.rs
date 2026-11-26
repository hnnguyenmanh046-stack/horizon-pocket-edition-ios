// A block was placed: block particles fly off the edges of its shape, as
// Particle Interactions' `spawnBlockPlaceParticle` does. The particles are the
// placed block's own (its break texture), queued for every player of the world.
fn block_place_particles_place(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    state: horizon_world::registry::BlockStateId,
) {
    use crate::game::native_block_place_particles as m;
    use horizon_client::particles::{ParticleOptions, ParticleType};

    if !game.native_on(m::ID) {
        return;
    }
    // Collect everything the facts say first: they borrow the game, and the
    // sending below needs it mutably.
    let points = {
        let facts = game.res.facts();
        let placed = facts.get(state);
        if m::skips(&placed.name) {
            return;
        }
        let Some(bounds) = m::bounds(&placed.outline) else {
            return;
        };
        let solid_at = |dy: i32| {
            let id = game.world.block(pos[0], pos[1] + dy, pos[2]);
            facts.get(id).legacy_solid
        };
        m::edge_points(bounds, m::PER_EDGE, solid_at(-1), solid_at(1))
    };
    // A block the game does not know has no particle texture.
    if game.pack.assets.registry.state(state).is_none() {
        return;
    }
    let options = ParticleOptions::simple(ParticleType::Block);
    let dimension = game.dim.id();
    for (offset, velocity) in points {
        let at = [
            f64::from(pos[0]) + offset[0],
            f64::from(pos[1]) + offset[1],
            f64::from(pos[2]) + offset[2],
        ];
        // Count 0: one particle whose direction is `spread`. This is what
        // `native_send_particles` queues, from the game rather than a level.
        game.lan_send_particles(crate::game::lan_particles::QueuedParticle {
            dimension,
            options: options.clone(),
            state: Some(state),
            at,
            spread: velocity,
            speed: 1.0,
            count: 0,
            randomization: 0,
        });
    }
}
