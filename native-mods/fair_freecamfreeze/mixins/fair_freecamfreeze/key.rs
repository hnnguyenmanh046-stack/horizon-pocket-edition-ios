// The freecam key went down: leave the body, or snap back to it.
fn freecam_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use crate::game::fair_freecam as fc;
    // Turned off in Options > Mods: nothing until the next build leaves it out.
    if !pressed || !game.native_on(fc::ID) || !game.native_playing() {
        return;
    }
    if key != game.native_mod_key(fc::ID, fc::KEY, fc::DEFAULT_KEY) {
        return;
    }
    // A touch button's key reaches the mods twice per press, so the toggle would
    // start and stop at once: a second press within 150 ms is the same press.
    if pressed {
        thread_local! {
            static LAST: std::cell::Cell<Option<std::time::Instant>> =
                const { std::cell::Cell::new(None) };
        }
        let now = std::time::Instant::now();
        let twice = LAST.get().is_some_and(|t| {
            now.duration_since(t) < std::time::Duration::from_millis(150)
        });
        if twice {
            return;
        }
        LAST.set(Some(now));
    }
    if fc::active() {
        fc::stop();
        return;
    }
    if game.player.is_none() {
        return;
    }
    let settings = {
        let game = &*game;
        fc::settings(|name| game.native_setting(fc::ID, name))
    };
    // Your own world (single player, or the LAN world you host) always allows it.
    if game.integrated.is_none() && !settings.servers {
        game.native_chat(fc::SERVER_OFF);
        return;
    }
    // Start where the view was last drawn from.
    fc::start(game.view.position, game.view.y_rot, game.view.x_rot);
}
