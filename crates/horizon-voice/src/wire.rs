//! What voice chat says on the game's connection: one custom payload, [`CHANNEL`], whose body
//! is one of the messages here.
//!
//! A player's game talks only to the host's ([`Up`]); the host's answers and passes voices on
//! ([`Down`]). The host gives every player in voice chat a short number ([`Down::Welcome`]) and
//! stamps it on what it passes on, so a frame doesn't carry sixteen bytes of who spoke it and
//! nobody can speak as someone else.
//!
//! Reading never fails loudly: bytes that aren't a message of this version are `None`, and
//! are dropped.

/// The custom payload voice chat travels in.
pub const CHANNEL: &str = "horizon:voice";
/// This format. A game that speaks another says so in [`Up::Join`] and gets no welcome.
pub const VERSION: u8 = 1;
/// The longest coded frame a message may carry (the codec's are under half of it).
pub const MAX_FRAME: usize = 512;
/// The most players a roster names (a world's players, with room to spare).
pub const MAX_ROSTER: usize = 64;
/// The longest name a roster carries, in bytes.
pub const MAX_NAME: usize = 48;

/// On a frame: the first of a stretch of speech (the listener starts a new one).
pub const START: u8 = 1;
/// On a frame: the last of a stretch of speech (nothing follows until the next start). It may
/// carry no sound.
pub const END: u8 = 2;

const JOIN: u8 = 0x01;
const STATE: u8 = 0x02;
const LISTEN: u8 = 0x03;
const FRAME_UP: u8 = 0x04;
const WELCOME: u8 = 0x81;
const ROSTER: u8 = 0x82;
const FRAME_DOWN: u8 = 0x84;

/// A player's game to the host's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Up<'a> {
    /// This game has voice chat, in this version: the host answers [`Down::Welcome`].
    Join { version: u8 },
    /// Voice chat is on here (the others' voices are wanted), and the microphone is live.
    State { enabled: bool, mic_live: bool },
    /// The players (by number) this one doesn't want to hear: muted here, so not sent.
    Listen { muted: Vec<u8> },
    /// One coded frame of this player's voice.
    Frame { seq: u16, flags: u8, opus: &'a [u8] },
}

/// One player, as the host tells the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The number the host stamps on this player's frames.
    pub id: u8,
    pub uuid: u128,
    pub name: String,
    /// Voice chat is on in their game.
    pub enabled: bool,
    /// Their microphone is live.
    pub mic_live: bool,
}

/// The host's game to a player's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Down<'a> {
    /// The host has voice chat; `id` is this player's number.
    Welcome { version: u8, id: u8 },
    /// Everyone in voice chat's reach (every player whose game has it), as of now.
    Roster { entries: Vec<Entry> },
    /// One coded frame of player `id`'s voice.
    Frame {
        id: u8,
        seq: u16,
        flags: u8,
        opus: &'a [u8],
    },
}

impl Up<'_> {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Join { version } => vec![JOIN, *version],
            Self::State { enabled, mic_live } => {
                vec![STATE, u8::from(*enabled) | (u8::from(*mic_live) << 1)]
            }
            Self::Listen { muted } => {
                let muted = &muted[..muted.len().min(usize::from(u8::MAX))];
                let mut out = Vec::with_capacity(2 + muted.len());
                out.push(LISTEN);
                out.push(muted.len() as u8);
                out.extend_from_slice(muted);
                out
            }
            Self::Frame { seq, flags, opus } => {
                let mut out = Vec::with_capacity(4 + opus.len());
                out.push(FRAME_UP);
                out.extend_from_slice(&seq.to_be_bytes());
                out.push(*flags);
                out.extend_from_slice(opus);
                out
            }
        }
    }
}

impl<'a> Up<'a> {
    pub fn decode(data: &'a [u8]) -> Option<Self> {
        let (kind, rest) = data.split_first()?;
        match *kind {
            JOIN => match rest {
                [version] => Some(Self::Join { version: *version }),
                _ => None,
            },
            STATE => match rest {
                [flags] if *flags < 4 => Some(Self::State {
                    enabled: flags & 1 != 0,
                    mic_live: flags & 2 != 0,
                }),
                _ => None,
            },
            LISTEN => {
                let (count, ids) = rest.split_first()?;
                (ids.len() == usize::from(*count)).then(|| Self::Listen {
                    muted: ids.to_vec(),
                })
            }
            FRAME_UP => {
                let (seq, flags, opus) = frame(rest)?;
                Some(Self::Frame { seq, flags, opus })
            }
            _ => None,
        }
    }
}

/// A frame's sequence number, flags and sound, checked.
fn frame(rest: &[u8]) -> Option<(u16, u8, &[u8])> {
    let [a, b, flags, opus @ ..] = rest else {
        return None;
    };
    if opus.len() > MAX_FRAME || *flags > (START | END) {
        return None;
    }
    // Only the end of a stretch may be empty.
    if opus.is_empty() && flags & END == 0 {
        return None;
    }
    Some((u16::from_be_bytes([*a, *b]), *flags, opus))
}

impl Down<'_> {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Welcome { version, id } => vec![WELCOME, *version, *id],
            Self::Roster { entries } => {
                let entries = &entries[..entries.len().min(MAX_ROSTER)];
                let mut out = Vec::with_capacity(2 + entries.len() * 32);
                out.push(ROSTER);
                out.push(entries.len() as u8);
                for e in entries {
                    out.push(e.id);
                    out.extend_from_slice(&e.uuid.to_be_bytes());
                    out.push(u8::from(e.enabled) | (u8::from(e.mic_live) << 1));
                    let name = clip(&e.name);
                    out.push(name.len() as u8);
                    out.extend_from_slice(name.as_bytes());
                }
                out
            }
            Self::Frame {
                id,
                seq,
                flags,
                opus,
            } => {
                let mut out = Vec::with_capacity(5 + opus.len());
                out.push(FRAME_DOWN);
                out.push(*id);
                out.extend_from_slice(&seq.to_be_bytes());
                out.push(*flags);
                out.extend_from_slice(opus);
                out
            }
        }
    }
}

/// `name`, cut to [`MAX_NAME`] bytes at a character's edge.
fn clip(name: &str) -> &str {
    let mut end = name.len().min(MAX_NAME);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

impl<'a> Down<'a> {
    pub fn decode(data: &'a [u8]) -> Option<Self> {
        let (kind, rest) = data.split_first()?;
        match *kind {
            WELCOME => match rest {
                [version, id] => Some(Self::Welcome {
                    version: *version,
                    id: *id,
                }),
                _ => None,
            },
            ROSTER => {
                let (count, mut rest) = rest.split_first()?;
                if usize::from(*count) > MAX_ROSTER {
                    return None;
                }
                let mut entries = Vec::with_capacity(usize::from(*count));
                for _ in 0..*count {
                    let (id, after) = rest.split_first()?;
                    let (uuid, after) = after.split_first_chunk::<16>()?;
                    let (flags, after) = after.split_first()?;
                    let (len, after) = after.split_first()?;
                    let len = usize::from(*len);
                    if *flags > 3 || len > MAX_NAME || after.len() < len {
                        return None;
                    }
                    let (name, after) = after.split_at(len);
                    entries.push(Entry {
                        id: *id,
                        uuid: u128::from_be_bytes(*uuid),
                        name: std::str::from_utf8(name).ok()?.to_owned(),
                        enabled: flags & 1 != 0,
                        mic_live: flags & 2 != 0,
                    });
                    rest = after;
                }
                rest.is_empty().then_some(Self::Roster { entries })
            }
            FRAME_DOWN => {
                let (id, rest) = rest.split_first()?;
                let (seq, flags, opus) = frame(rest)?;
                Some(Self::Frame {
                    id: *id,
                    seq,
                    flags,
                    opus,
                })
            }
            _ => None,
        }
    }
}

/// A frame as the host passes it on: the speaker's number put before what the speaker sent
/// (`up`, a whole [`Up::Frame`] message), without reading the sound. None when `up` isn't one.
pub fn relay(id: u8, up: &[u8]) -> Option<Vec<u8>> {
    let (kind, rest) = up.split_first()?;
    if *kind != FRAME_UP {
        return None;
    }
    frame(rest)?;
    let mut out = Vec::with_capacity(up.len() + 1);
    out.push(FRAME_DOWN);
    out.push(id);
    out.extend_from_slice(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u8, name: &str) -> Entry {
        Entry {
            id,
            uuid: 0x0123_4567_89ab_cdef_0011_2233_4455_6677 + u128::from(id),
            name: name.to_owned(),
            enabled: id % 2 == 0,
            mic_live: id % 3 == 0,
        }
    }

    #[test]
    fn every_message_reads_back() {
        let sound = [7u8; 80];
        for up in [
            Up::Join { version: VERSION },
            Up::State {
                enabled: true,
                mic_live: false,
            },
            Up::State {
                enabled: true,
                mic_live: true,
            },
            Up::Listen { muted: vec![] },
            Up::Listen {
                muted: vec![1, 2, 200],
            },
            Up::Frame {
                seq: 65_535,
                flags: START,
                opus: &sound,
            },
            Up::Frame {
                seq: 3,
                flags: END,
                opus: &[],
            },
        ] {
            assert_eq!(Up::decode(&up.encode()), Some(up));
        }
        for down in [
            Down::Welcome {
                version: VERSION,
                id: 9,
            },
            Down::Roster { entries: vec![] },
            Down::Roster {
                entries: vec![entry(0, "Alex"), entry(3, "Stève_2"), entry(6, "")],
            },
            Down::Frame {
                id: 4,
                seq: 513,
                flags: 0,
                opus: &sound,
            },
        ] {
            assert_eq!(Down::decode(&down.encode()), Some(down));
        }
    }

    #[test]
    fn the_host_passes_a_frame_on_under_the_speakers_number() {
        let sound = [9u8; 61];
        let up = Up::Frame {
            seq: 77,
            flags: START,
            opus: &sound,
        }
        .encode();
        let down = relay(5, &up).unwrap();
        assert_eq!(
            Down::decode(&down),
            Some(Down::Frame {
                id: 5,
                seq: 77,
                flags: START,
                opus: &sound,
            })
        );
        // Only frames are passed on, and only well-formed ones.
        assert_eq!(relay(5, &Up::Join { version: 1 }.encode()), None);
        assert_eq!(relay(5, &[FRAME_UP, 0, 1, 0]), None);
        assert_eq!(relay(5, &[FRAME_UP, 0, 1, 8, 1]), None);
        assert_eq!(relay(5, &[]), None);
        let mut long = vec![FRAME_UP, 0, 1, 0];
        long.resize(4 + MAX_FRAME + 1, 1);
        assert_eq!(relay(5, &long), None);
    }

    #[test]
    fn a_long_name_is_cut_at_a_character_and_a_long_roster_at_its_limit() {
        let name = "é".repeat(40);
        let down = Down::Roster {
            entries: vec![entry(1, &name)],
        }
        .encode();
        let Some(Down::Roster { entries }) = Down::decode(&down) else {
            panic!("the roster doesn't read");
        };
        assert_eq!(entries[0].name, "é".repeat(MAX_NAME / 2));
        let many: Vec<Entry> = (0..200u8).map(|i| entry(i, "P")).collect();
        let Some(Down::Roster { entries }) = Down::decode(&Down::Roster { entries: many }.encode())
        else {
            panic!("the roster doesn't read");
        };
        assert_eq!(entries.len(), MAX_ROSTER);
    }

    /// Bytes that aren't a message are nothing, whatever they are: every prefix of every
    /// message, every message with one byte changed, and noise.
    #[test]
    fn whatever_isnt_a_message_is_dropped_without_a_word() {
        let sound = [3u8; 40];
        let messages: Vec<Vec<u8>> = vec![
            Up::Join { version: VERSION }.encode(),
            Up::State {
                enabled: true,
                mic_live: true,
            }
            .encode(),
            Up::Listen {
                muted: vec![1, 2, 3],
            }
            .encode(),
            Up::Frame {
                seq: 1,
                flags: 0,
                opus: &sound,
            }
            .encode(),
            Down::Welcome {
                version: VERSION,
                id: 1,
            }
            .encode(),
            Down::Roster {
                entries: vec![entry(1, "Alex"), entry(2, "Steve")],
            }
            .encode(),
            Down::Frame {
                id: 1,
                seq: 1,
                flags: 0,
                opus: &sound,
            }
            .encode(),
        ];
        for message in &messages {
            for cut in 0..message.len() {
                let _ = Up::decode(&message[..cut]);
                let _ = Down::decode(&message[..cut]);
                let _ = relay(1, &message[..cut]);
            }
            for at in 0..message.len() {
                for bit in 0..8 {
                    let mut changed = message.clone();
                    changed[at] ^= 1 << bit;
                    let _ = Up::decode(&changed);
                    let _ = Down::decode(&changed);
                    let _ = relay(1, &changed);
                }
            }
        }
        let mut seed = 99u32;
        for len in 0..600 {
            let noise: Vec<u8> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 24) as u8
                })
                .collect();
            let _ = Up::decode(&noise);
            let _ = Down::decode(&noise);
        }
        // What must be refused is: flags no game sets, and a frame with no sound that isn't an
        // end.
        assert_eq!(Up::decode(&[STATE, 4]), None);
        assert_eq!(Up::decode(&[FRAME_UP, 0, 0, 0]), None);
        assert_eq!(Up::decode(&[FRAME_UP, 0, 0, 4, 1]), None);
        assert_eq!(Up::decode(&[LISTEN, 2, 1]), None);
        assert_eq!(Down::decode(&[WELCOME, 1]), None);
        assert_eq!(Down::decode(&[ROSTER, 1, 0]), None);
    }
}
