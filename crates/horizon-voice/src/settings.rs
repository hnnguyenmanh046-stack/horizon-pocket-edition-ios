//! What a player chose for voice chat, kept between sessions: which microphone, how loud
//! everyone is, and each player's own volume and mute, by account.
//!
//! Whether voice chat is on, and whether the microphone is, are never kept: every world is
//! joined with both off.
use crate::engine::MAX_VOLUME;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// How the others' voices are kept out of the microphone when they come out of the phone's
/// own speaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Echo {
    /// The system's echo canceller on the speaker, nothing on headphones.
    #[default]
    Auto,
    /// The system's echo canceller, always.
    System,
    /// The microphone turned down while the others are heard ([`crate::dsp::EchoGuard`]).
    Guard,
    /// Nothing.
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// The microphone chosen, as the device names it; empty for the automatic choice.
    pub mic: String,
    /// Everyone's volume together, 0 to 2.
    pub voices: f32,
    /// A player's own volume, by account (32 hex digits), where it isn't 1.
    pub volumes: BTreeMap<String, f32>,
    /// The players muted here, by account.
    pub muted: BTreeSet<String>,
    pub echo: Echo,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            mic: String::new(),
            voices: 1.0,
            volumes: BTreeMap::new(),
            muted: BTreeSet::new(),
            echo: Echo::Auto,
        }
    }
}

/// The most players whose volume is kept: the ones set last stay.
const MOST: usize = 500;

fn volume(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, MAX_VOLUME)
    } else {
        1.0
    }
}

fn account(uuid: &str) -> bool {
    uuid.len() == 32 && uuid.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Settings {
    /// The settings a file holds. A file that doesn't read gives the defaults; what no game
    /// would have written in one that does (a volume out of range, a name that is no
    /// account) is put right or left out.
    pub fn parse(text: &str) -> Self {
        let mut settings: Self = serde_json::from_str(text).unwrap_or_default();
        settings.version = 1;
        settings.voices = volume(settings.voices);
        let end = (0..=settings.mic.len().min(200))
            .rev()
            .find(|i| settings.mic.is_char_boundary(*i))
            .unwrap_or(0);
        settings.mic.truncate(end);
        settings.volumes.retain(|uuid, _| account(uuid));
        for v in settings.volumes.values_mut() {
            *v = volume(*v);
        }
        settings.muted.retain(|uuid| account(uuid));
        settings
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// A player's volume here (1 unless set).
    pub fn volume_of(&self, uuid: &str) -> f32 {
        self.volumes.get(uuid).copied().unwrap_or(1.0)
    }

    pub fn is_muted(&self, uuid: &str) -> bool {
        self.muted.contains(uuid)
    }

    /// Sets a player's volume; true when that changed anything.
    pub fn set_volume(&mut self, uuid: &str, to: f32) -> bool {
        let to = volume(to);
        if !account(uuid) || self.volume_of(uuid) == to {
            return false;
        }
        if to == 1.0 {
            self.volumes.remove(uuid);
        } else {
            if self.volumes.len() >= MOST && !self.volumes.contains_key(uuid) {
                self.volumes.pop_first();
            }
            self.volumes.insert(uuid.to_owned(), to);
        }
        true
    }

    /// Mutes a player here, or not; true when that changed anything.
    pub fn set_muted(&mut self, uuid: &str, muted: bool) -> bool {
        if !account(uuid) {
            return false;
        }
        if muted {
            if self.muted.len() >= MOST && !self.muted.contains(uuid) {
                self.muted.pop_first();
            }
            self.muted.insert(uuid.to_owned())
        } else {
            self.muted.remove(uuid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALEX: &str = "0123456789abcdef0123456789abcdef";
    const STEVE: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn what_was_chosen_is_kept_and_what_is_as_it_comes_isnt_written() {
        let mut settings = Settings::default();
        assert_eq!(settings.volume_of(ALEX), 1.0);
        assert!(settings.set_volume(ALEX, 1.5));
        assert!(!settings.set_volume(ALEX, 1.5));
        assert!(settings.set_volume(STEVE, 0.25));
        assert!(settings.set_muted(STEVE, true));
        assert!(!settings.set_muted(STEVE, true));
        settings.mic = "Headset Microphone".into();
        settings.voices = 0.8;
        settings.echo = Echo::Guard;
        let back = Settings::parse(&settings.to_json());
        assert_eq!(back, settings);
        assert_eq!(back.volume_of(ALEX), 1.5);
        assert!(back.is_muted(STEVE) && !back.is_muted(ALEX));
        // Back to as it comes: not kept.
        assert!(settings.set_volume(ALEX, 1.0));
        assert!(!settings.volumes.contains_key(ALEX));
        assert!(settings.set_muted(STEVE, false));
        assert!(settings.muted.is_empty());
        // Whether voice chat or the microphone is on is nowhere in it.
        let text = settings.to_json();
        assert!(!text.contains("enabled") && !text.contains("live"));
    }

    #[test]
    fn a_file_that_is_wrong_gives_the_defaults_for_what_is_wrong() {
        assert_eq!(Settings::parse(""), Settings::default());
        assert_eq!(Settings::parse("not json"), Settings::default());
        assert_eq!(Settings::parse("{}"), Settings::default());
        let odd = Settings::parse(
            r#"{"version":9,"voices":50,"volumes":{"0123456789abcdef0123456789abcdef":-3,"x":1.5},
                "muted":["nobody","fedcba9876543210fedcba9876543210"],"echo":"guard","new":true}"#,
        );
        assert_eq!(odd.version, 1);
        assert_eq!(odd.voices, MAX_VOLUME);
        assert_eq!(odd.volumes.len(), 1);
        assert_eq!(odd.volume_of(ALEX), 0.0);
        assert_eq!(odd.muted.len(), 1);
        assert!(odd.is_muted(STEVE));
        assert_eq!(odd.echo, Echo::Guard);
        // A name that isn't an account is never kept.
        let mut settings = Settings::default();
        assert!(!settings.set_volume("Alex", 0.5));
        assert!(!settings.set_muted("Alex", true));
        assert!(settings.set_volume(ALEX, f32::NAN) || settings.volume_of(ALEX) == 1.0);
        assert!(settings.volume_of(ALEX).is_finite());
    }

    #[test]
    fn only_so_many_players_are_kept() {
        let mut settings = Settings::default();
        for i in 0..MOST + 50 {
            let uuid = format!("{i:032x}");
            assert!(settings.set_volume(&uuid, 0.5));
            assert!(settings.set_muted(&uuid, true));
        }
        assert_eq!(settings.volumes.len(), MOST);
        assert_eq!(settings.muted.len(), MOST);
    }
}
