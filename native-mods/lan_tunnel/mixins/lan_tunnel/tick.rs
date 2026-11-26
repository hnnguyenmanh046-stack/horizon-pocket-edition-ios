// Once a tick, in the window's game: a world open to LAN gets a tunnel to a
// public address, and the address is said in chat when it is known.
fn lan_tunnel_tick(game: &mut crate::game::Game<'_>) {
    use crate::game::native_lan_tunnel as m;
    if !game.native_window() {
        return;
    }
    // Off in Options > Mods: the tunnel closes like a closed LAN does.
    let lan = if game.native_on(m::ID) { game.lan_port() } else { None };
    match m::poll(lan) {
        Some(m::Note::Address(address)) => {
            game.native_chat(&format!("LAN Tunnel: friends join at {address}"));
        }
        Some(m::Note::Failed(why)) => {
            game.native_chat(&format!("LAN Tunnel: no public address ({why})"));
        }
        None => {}
    }
}
