// LAN Tunnel's row in Options > Mods: the player turns it off there.
fn lan_tunnel_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::native_lan_tunnel as m;
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: Vec::new(),
    });
}
