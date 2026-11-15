//! Voice chat's relay on the host: every player's game talks to the host's, and the host's
//! passes each voice on to the players who want to hear it (`horizon_voice::wire`).
//!
//! It runs on the connections' own threads, not on the game's tick: a frame read from one
//! player goes straight to the others' writers ([`PlayerSender::send_now`]), ahead of whatever
//! world data is queued for them. A world that is paused, saving or waiting for its storage
//! still carries voices.
//!
//! The host decides who is who (a frame is stamped with its speaker's number here, whatever
//! the sender wrote), passes on only what was asked for (a player with voice chat off gets
//! the list of players, never a voice; a speaker a listener muted isn't sent to them), and
//! keeps anyone from flooding the others ([`RATE`], [`TALKERS`]). Nothing here ever ends a
//! connection: what isn't right is dropped.
use super::conn::PlayerSender;
use horizon_protocol::packets::{configuration::CustomPayload, play::Clientbound};
use horizon_voice::wire::{self, CHANNEL, Down, END, Entry, Up, VERSION};
use std::{
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Whether the next world opened carries voices: a shared world's host sets it before the
/// world opens (its own game joins first, and must hear the offer too).
static WANTED: AtomicBool = AtomicBool::new(false);

pub fn set_wanted(on: bool) {
    WANTED.store(on, Ordering::SeqCst);
}

pub fn wanted() -> bool {
    WANTED.load(Ordering::SeqCst)
}

/// Frames a second one player may send (a voice is 50), and how many may come together
/// after a stall.
const RATE: f32 = 55.0;
const BURST: f32 = 100.0;
/// Voices passed on at once: the first to speak keep the floor, a fifth waits for one to end.
const TALKERS: usize = 4;
/// A voice with no frame for this long has stopped, said so or not.
const QUIET: Duration = Duration::from_millis(400);

struct Peer {
    conn: u32,
    uuid: u128,
    name: String,
    sender: PlayerSender,
    /// Its number, once its game said it has voice chat.
    id: Option<u8>,
    enabled: bool,
    mic_live: bool,
    /// The speakers it doesn't want.
    unwanted: Vec<u8>,
    tokens: f32,
    refilled: Instant,
    /// Since when it has been speaking, and its last frame.
    since: Option<Instant>,
    last: Instant,
}

/// What the configuration announces to a Horizon client: voice chat, in this version.
pub fn offer() -> Vec<u8> {
    vec![VERSION]
}

/// A voice message as the play packet that carries it.
fn packet(data: Vec<u8>) -> Vec<u8> {
    Clientbound::CustomPayload(CustomPayload {
        channel: CHANNEL.into(),
        data,
    })
    .encode()
}

pub struct Relay {
    on: bool,
    peers: Mutex<Vec<Peer>>,
}

impl Relay {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            peers: Mutex::new(Vec::new()),
        }
    }

    /// This world carries voices.
    pub fn on(&self) -> bool {
        self.on
    }

    fn peers(&self) -> MutexGuard<'_, Vec<Peer>> {
        self.peers.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A Horizon client started playing. It is in voice chat's reach once its game says so
    /// ([`Up::Join`]); a splitscreen seat never does.
    pub fn enter(&self, conn: u32, uuid: u128, name: &str, sender: PlayerSender) {
        if !self.on {
            return;
        }
        let now = Instant::now();
        let mut peers = self.peers();
        peers.retain(|p| p.conn != conn);
        peers.push(Peer {
            conn,
            uuid,
            name: name.to_owned(),
            sender,
            id: None,
            enabled: false,
            mic_live: false,
            unwanted: Vec::new(),
            tokens: BURST,
            refilled: now,
            since: None,
            last: now,
        });
    }

    /// The connection ended.
    pub fn leave(&self, conn: u32) {
        let mut peers = self.peers();
        let Some(at) = peers.iter().position(|p| p.conn == conn) else {
            return;
        };
        let was_in = peers.remove(at).id.is_some();
        if was_in {
            tell_everyone(&peers);
        }
    }

    /// What player `conn`'s game sent on the voice channel.
    pub fn handle(&self, conn: u32, data: &[u8]) {
        self.handle_at(conn, data, Instant::now());
    }

    fn handle_at(&self, conn: u32, data: &[u8], now: Instant) {
        if !self.on {
            return;
        }
        let Some(message) = Up::decode(data) else {
            return;
        };
        let mut peers = self.peers();
        let Some(at) = peers.iter().position(|p| p.conn == conn) else {
            return;
        };
        match message {
            Up::Join { version } => {
                // Another version of the messages: no welcome, and that game says so.
                if version != VERSION {
                    return;
                }
                let id = match peers[at].id {
                    Some(id) => id,
                    None => {
                        let Some(free) =
                            (0..=u8::MAX).find(|id| peers.iter().all(|p| p.id != Some(*id)))
                        else {
                            return;
                        };
                        peers[at].id = Some(free);
                        free
                    }
                };
                peers[at].sender.send_now(packet(
                    Down::Welcome {
                        version: VERSION,
                        id,
                    }
                    .encode(),
                ));
                tell_everyone(&peers);
            }
            Up::State { enabled, mic_live } => {
                let peer = &mut peers[at];
                if peer.id.is_none() {
                    return;
                }
                // A microphone is live only with voice chat on.
                let mic_live = enabled && mic_live;
                if (peer.enabled, peer.mic_live) == (enabled, mic_live) {
                    return;
                }
                peer.enabled = enabled;
                peer.mic_live = mic_live;
                if !mic_live {
                    peer.since = None;
                }
                tell_everyone(&peers);
            }
            Up::Listen { muted } => peers[at].unwanted = muted,
            Up::Frame { flags, .. } => {
                let peer = &mut peers[at];
                let Some(id) = peer.id else {
                    return;
                };
                if !peer.mic_live {
                    return;
                }
                // No more than a voice's worth, with room for what a stall held back.
                let waited = now.saturating_duration_since(peer.refilled).as_secs_f32();
                peer.tokens = (peer.tokens + waited * RATE).min(BURST);
                peer.refilled = now;
                if peer.tokens < 1.0 {
                    return;
                }
                peer.tokens -= 1.0;
                // The floor: whoever has been speaking longest keeps it.
                let since = match peer.since {
                    Some(since) if now.saturating_duration_since(peer.last) < QUIET => since,
                    _ => now,
                };
                peer.since = if flags & END != 0 { None } else { Some(since) };
                peer.last = now;
                let ahead = peers
                    .iter()
                    .filter(|p| p.conn != conn && p.mic_live)
                    .filter(|p| now.saturating_duration_since(p.last) < QUIET)
                    .filter(|p| p.since.is_some_and(|other| (other, p.conn) < (since, conn)))
                    .count();
                if ahead >= TALKERS {
                    return;
                }
                let Some(passed) = wire::relay(id, data) else {
                    return;
                };
                let body = packet(passed);
                for listener in peers.iter() {
                    if listener.conn != conn
                        && listener.id.is_some()
                        && listener.enabled
                        && !listener.unwanted.contains(&id)
                    {
                        listener.sender.send_now(body.clone());
                    }
                }
            }
        }
    }
}

/// Everyone in reach hears who is in reach, with voice chat on or off: a game with it off
/// tells its player that others are using it.
fn tell_everyone(peers: &[Peer]) {
    let entries: Vec<Entry> = peers
        .iter()
        .filter_map(|p| {
            Some(Entry {
                id: p.id?,
                uuid: p.uuid,
                name: p.name.clone(),
                enabled: p.enabled,
                mic_live: p.mic_live,
            })
        })
        .collect();
    let body = packet(Down::Roster { entries }.encode());
    for peer in peers.iter().filter(|p| p.id.is_some()) {
        peer.sender.send_now(body.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lan::Out;
    use std::sync::{
        Arc,
        mpsc::{Receiver, channel},
    };

    struct Player {
        conn: u32,
        sender: PlayerSender,
        out: Receiver<Out>,
    }

    impl Player {
        fn new(relay: &Relay, conn: u32, name: &str) -> Self {
            let (tx, out) = channel();
            let sender = PlayerSender::new(tx, Arc::default());
            relay.enter(conn, u128::from(conn) << 64, name, sender.clone());
            Self { conn, sender, out }
        }

        fn say(&self, relay: &Relay, up: &Up<'_>, now: Instant) {
            relay.handle_at(self.conn, &up.encode(), now);
        }

        /// Joins voice chat, and with `enabled` turns it on.
        fn join(&self, relay: &Relay, enabled: bool, mic_live: bool) {
            let now = Instant::now();
            self.say(relay, &Up::Join { version: VERSION }, now);
            if enabled {
                self.say(relay, &Up::State { enabled, mic_live }, now);
            }
        }

        /// The voice messages sent to this player since the last look, in order: nothing
        /// else was sent, and nothing went by the game's own queue.
        fn got(&self) -> Vec<Vec<u8>> {
            let prefix = packet(Vec::new());
            let mut messages = Vec::new();
            while let Ok(out) = self.out.try_recv() {
                assert!(
                    matches!(out, Out::Express),
                    "voice went by the game's queue"
                );
            }
            while let Some(body) = self.sender.take_express() {
                assert!(body.starts_with(&prefix));
                messages.push(body[prefix.len()..].to_vec());
            }
            messages
        }

        fn frames(&self) -> Vec<(u8, u16)> {
            self.got()
                .iter()
                .filter_map(|m| match Down::decode(m) {
                    Some(Down::Frame { id, seq, .. }) => Some((id, seq)),
                    _ => None,
                })
                .collect()
        }

        fn roster(&self) -> Option<Vec<Entry>> {
            self.got().iter().rev().find_map(|m| match Down::decode(m) {
                Some(Down::Roster { entries }) => Some(entries),
                _ => None,
            })
        }
    }

    fn frame(seq: u16, flags: u8) -> Up<'static> {
        Up::Frame {
            seq,
            flags,
            opus: if flags & END != 0 { &[] } else { &[7; 60] },
        }
    }

    #[test]
    fn a_voice_goes_to_those_who_asked_for_it_and_the_list_to_everyone() {
        let relay = Relay::new(true);
        let alex = Player::new(&relay, 1, "Alex");
        let steve = Player::new(&relay, 2, "Steve");
        let sam = Player::new(&relay, 3, "Sam");
        // A splitscreen seat: a Horizon client that never says it has voice chat.
        let seat = Player::new(&relay, 4, "Seat");
        alex.join(&relay, true, true);
        // Alex is welcomed under a number and hears the list.
        let got = alex.got();
        assert_eq!(
            Down::decode(&got[0]),
            Some(Down::Welcome {
                version: VERSION,
                id: 0
            })
        );
        steve.join(&relay, true, false);
        sam.join(&relay, false, false);
        // Sam has voice chat off and still knows who is using it.
        let list = sam.roster().unwrap();
        assert_eq!(
            list.iter()
                .map(|e| (e.id, e.name.as_str(), e.enabled, e.mic_live))
                .collect::<Vec<_>>(),
            [
                (0, "Alex", true, true),
                (1, "Steve", true, false),
                (2, "Sam", false, false)
            ]
        );
        assert_eq!(list[1].uuid, 2u128 << 64);
        alex.got();
        steve.got();
        let now = Instant::now();
        for seq in 0..5 {
            alex.say(&relay, &frame(seq, 0), now);
        }
        // Steve hears Alex, under Alex's number; Alex doesn't hear themself; Sam and the
        // seat hear nothing.
        assert_eq!(steve.frames(), (0..5).map(|s| (0, s)).collect::<Vec<_>>());
        assert!(alex.got().is_empty());
        assert!(sam.got().is_empty());
        assert!(seat.got().is_empty());
        // Steve's microphone is off: what his game sends anyway goes nowhere.
        steve.say(&relay, &frame(0, 0), now);
        assert!(alex.got().is_empty());
        // Sam turns voice chat on: everyone hears so, and Sam hears Alex from then on.
        sam.say(
            &relay,
            &Up::State {
                enabled: true,
                mic_live: false,
            },
            now,
        );
        assert!(alex.roster().unwrap()[2].enabled);
        sam.got();
        alex.say(&relay, &frame(5, 0), now);
        assert_eq!(sam.frames(), [(0, 5)]);
        // Sam mutes Alex: no longer sent.
        sam.say(&relay, &Up::Listen { muted: vec![0] }, now);
        alex.say(&relay, &frame(6, 0), now);
        assert!(sam.got().is_empty());
        assert_eq!(steve.frames(), [(0, 5), (0, 6)]);
        // Alex leaves: the others hear so, and the number is free for the next player.
        relay.leave(alex.conn);
        assert_eq!(steve.roster().unwrap().len(), 2);
        let late = Player::new(&relay, 9, "Late");
        late.join(&relay, true, true);
        assert_eq!(
            Down::decode(&late.got()[0]),
            Some(Down::Welcome {
                version: VERSION,
                id: 0
            })
        );
    }

    #[test]
    fn nobody_speaks_as_someone_else_or_floods_the_others() {
        let relay = Relay::new(true);
        let alex = Player::new(&relay, 1, "Alex");
        let steve = Player::new(&relay, 2, "Steve");
        alex.join(&relay, true, true);
        steve.join(&relay, true, true);
        steve.got();
        let start = Instant::now();
        // A frame made to look like one passed on from Steve: not a message a player sends.
        let forged = Down::Frame {
            id: 1,
            seq: 0,
            flags: 0,
            opus: &[1; 20],
        }
        .encode();
        relay.handle_at(alex.conn, &forged, start);
        relay.handle_at(alex.conn, &[], start);
        relay.handle_at(alex.conn, &[0xff; 700], start);
        relay.handle_at(77, &frame(0, 0).encode(), start);
        assert!(steve.got().is_empty());
        // A flood: what a stall could have held back goes through, the rest is dropped, and
        // a second later a second's worth goes through again. Nobody is disconnected.
        // (Steve's game reads as they come: the lane to a player who doesn't holds only the
        // last half second.)
        let mut passed = 0;
        for seq in 0..1_000 {
            alex.say(&relay, &frame(seq, 0), start);
            passed += steve.frames().len();
        }
        assert_eq!(passed, BURST as usize);
        let later = start + Duration::from_secs(1);
        let mut passed = 0;
        for seq in 0..1_000 {
            alex.say(&relay, &frame(seq, 0), later);
            passed += steve.frames().len();
        }
        assert_eq!(passed, RATE as usize);
        // A listener who doesn't read is sent on to all the same, and holds only so much.
        let idle = Player::new(&relay, 6, "Idle");
        idle.join(&relay, true, false);
        idle.got();
        let much_later = later + Duration::from_secs(10);
        for seq in 0..60 {
            alex.say(&relay, &frame(seq, 0), much_later);
        }
        let held: Vec<u16> = idle.frames().into_iter().map(|(_, seq)| seq).collect();
        assert_eq!(held, (35..60).collect::<Vec<u16>>());
        steve.got();
        // A game of another version of the messages isn't welcomed, and isn't on the list.
        let old = Player::new(&relay, 5, "Old");
        old.say(
            &relay,
            &Up::Join {
                version: VERSION + 1,
            },
            start,
        );
        assert!(old.got().is_empty());
        old.say(
            &relay,
            &Up::State {
                enabled: true,
                mic_live: true,
            },
            start,
        );
        old.say(&relay, &frame(0, 0), later + Duration::from_secs(5));
        assert!(steve.got().is_empty());
    }

    #[test]
    fn the_first_four_voices_keep_the_floor_and_a_fifth_gets_it_when_one_stops() {
        let relay = Relay::new(true);
        let players: Vec<Player> = (0..6)
            .map(|i| {
                let player = Player::new(&relay, i + 1, &format!("P{i}"));
                player.join(&relay, true, i < 5);
                player
            })
            .collect();
        let listener = &players[5];
        listener.got();
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // Five start one after another, a frame each every 20 ms (the listener reads as they
        // come).
        let mut heard = Vec::new();
        for tick in 0..10u16 {
            for (i, player) in players[..5].iter().enumerate() {
                let now = at(u64::from(tick) * 20 + i as u64);
                player.say(&relay, &frame(tick, 0), now);
            }
            heard.extend(listener.frames());
        }
        assert_eq!(heard.len(), 40);
        assert!(
            heard.iter().all(|(id, _)| *id < 4),
            "the fifth was passed on"
        );
        // The first ends: from then on the fifth is heard.
        players[0].say(&relay, &frame(10, END), at(200));
        for tick in 11..15u16 {
            for (i, player) in players[1..5].iter().enumerate() {
                player.say(&relay, &frame(tick, 0), at(u64::from(tick) * 20 + i as u64));
            }
        }
        let heard = listener.frames();
        assert_eq!(heard[0], (0, 10));
        assert_eq!(heard.iter().filter(|(id, _)| *id == 4).count(), 4);
        // One who just stops (no end said) loses the floor after a moment.
        for tick in 40..45u16 {
            for (i, player) in players[2..5].iter().enumerate() {
                player.say(&relay, &frame(tick, 0), at(u64::from(tick) * 20 + i as u64));
            }
        }
        players[0].say(&relay, &frame(50, 0), at(900));
        assert!(listener.frames().contains(&(0, 50)));
    }

    #[test]
    fn a_world_that_isnt_a_shared_world_carries_no_voices() {
        let relay = Relay::new(false);
        assert!(!relay.on());
        let alex = Player::new(&relay, 1, "Alex");
        let steve = Player::new(&relay, 2, "Steve");
        alex.join(&relay, true, true);
        steve.join(&relay, true, true);
        alex.say(&relay, &frame(0, 0), Instant::now());
        assert!(alex.got().is_empty() && steve.got().is_empty());
        relay.leave(alex.conn);
        // The next world opened is one or not as the host's game says before it opens.
        set_wanted(true);
        assert!(wanted());
        set_wanted(false);
        assert!(!wanted());
        assert_eq!(offer(), [VERSION]);
    }
}
