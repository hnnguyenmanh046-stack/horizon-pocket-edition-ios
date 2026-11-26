// Item Physic Lite's row in Options > Mods: the player turns it off there.
fn item_physic_lite_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::native_item_physic_lite as m;
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: Vec::new(),
    });
}
