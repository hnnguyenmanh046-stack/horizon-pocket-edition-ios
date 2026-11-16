//! Horizon PE: Shared Worlds + Mods: the keys of the native mods this edition is built with that
//! a touch button or a controller presses. Zoom's key is held (the touch controls' zoom button,
//! the controller's View button); Waymap's two are taps (the waypoint buttons, the D-pad's left
//! and down).
//!
//! A mod reads its key through `native_mod_key` (the player's choice in Options > Mods, else its
//! default), so what is pressed here is that same key by name: the mod can't tell it from a
//! keyboard's. The buttons are there only while their mod is in the app and turned on.
use super::Game;

/// A native mod's key: the mod, the key's name in it, and the key pressed when the player
/// chose none.
pub(super) struct ModKey {
    pub mod_id: &'static str,
    pub name: &'static str,
    pub default: &'static str,
}

/// Zoom: hold to zoom in (`key.zoom.zoom`, Z).
pub(super) const ZOOM: ModKey = ModKey {
    mod_id: "zoom",
    name: "zoom",
    default: "key.keyboard.z",
};
/// Waymap: the list of waypoints. M when none is chosen: Waymap takes M as well as its own
/// default.
pub(super) const WAYPOINTS: ModKey = ModKey {
    mod_id: "waymap",
    name: "panel",
    default: "key.keyboard.m",
};
/// Waymap: a waypoint here (N).
pub(super) const ADD_WAYPOINT: ModKey = ModKey {
    mod_id: "waymap",
    name: "add",
    default: "key.keyboard.n",
};

/// What is held, and which of the mods are in this app.
#[derive(Default)]
pub(super) struct ModKeys {
    /// Who holds which key down: the touch button or the controller, and the key as it was
    /// pressed (let go of by that name, whatever the player chooses meanwhile).
    held: Vec<(&'static str, String)>,
    /// The ids of the native mods in this app, asked once.
    built: Option<Vec<String>>,
}

impl Game<'_> {
    /// Whether native mod `id` is in this app and left on in Options > Mods.
    pub(super) fn mod_key_ready(&mut self, id: &str) -> bool {
        if self.mod_keys.built.is_none() {
            let mut mods = Vec::new();
            self.native_mods(&mut mods);
            self.mod_keys.built = Some(mods.into_iter().map(|m| m.id).collect());
        }
        let built = self.mod_keys.built.as_ref();
        built.is_some_and(|ids| ids.iter().any(|m| m == id)) && self.native_on(id)
    }

    /// `key` goes down or up for `holder` ("touch", "pad"). A key two hold is down until both
    /// let go; one that isn't held isn't let go of.
    pub(super) fn mod_key_hold(&mut self, holder: &'static str, key: &ModKey, down: bool) {
        let at = self.mod_keys.held.iter().position(|(h, _)| *h == holder);
        match (down, at) {
            (true, None) => {
                if !self.mod_key_ready(key.mod_id) {
                    return;
                }
                let name = self.native_mod_key(key.mod_id, key.name, key.default);
                let already = self.mod_keys.held.iter().any(|(_, n)| *n == name);
                self.mod_keys.held.push((holder, name.clone()));
                if !already {
                    self.mod_key_send(&name, true);
                }
            }
            (false, Some(i)) => {
                let (_, name) = self.mod_keys.held.remove(i);
                if !self.mod_keys.held.iter().any(|(_, n)| *n == name) {
                    self.mod_key_send(&name, false);
                }
            }
            _ => {}
        }
    }

    /// `key` pressed and let go.
    pub(super) fn mod_key_tap(&mut self, key: &ModKey) {
        if !self.mod_key_ready(key.mod_id) {
            return;
        }
        let name = self.native_mod_key(key.mod_id, key.name, key.default);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        self.android_custom_token_tap(&name);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.mod_key_send(&name, true);
            self.mod_key_send(&name, false);
        }
    }

    /// The key by its name (`key.keyboard.z`), as a keyboard's: on a phone the way a custom
    /// touch button's key goes, so the game and the mods both hear it.
    fn mod_key_send(&mut self, name: &str, down: bool) {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        self.android_custom_token_edge(name, down);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Some(code) = horizon_client::keybinds::key_code(name) {
            self.native_key_event(code, down, false);
        }
    }
}
