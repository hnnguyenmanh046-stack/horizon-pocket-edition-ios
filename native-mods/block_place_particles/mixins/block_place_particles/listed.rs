// Block Place Particles' row in Options > Mods: the player turns it off there.
fn block_place_particles_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::native_block_place_particles as m;
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: Vec::new(),
    });
}
