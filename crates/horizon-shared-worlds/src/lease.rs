//! The lease: the one record that says who hosts a shared world, who is playing and which copy
//! of the save is the newest.
//!
//! Every change is a conditional write on the version that was read, so of two games that try
//! to take the world at once the storage lets exactly one through, and a host that was replaced
//! can't write again: its next write is refused and it learns it is [`Error::Fenced`].
//!
//! Nothing here compares clocks between devices. A host that stops is noticed by its heartbeat
//! counter standing still on the watcher's own timer ([`Watch`], [`DEAD`]), and a host that
//! can't reach the storage freezes its world well before that ([`Pulse`], [`SELF_FENCE`]), so by
//! the time anyone may take over, the old host has already stopped changing the world.
use crate::{
    Error, Result,
    codec::{hex, random},
    manifest::GenId,
    store::{Condition, Etag, Store},
};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// The record's format. A lease written by a newer format is never touched.
pub const VERSION: u32 = 1;
/// How often a host rewrites the lease.
pub const HEARTBEAT: Duration = Duration::from_secs(6);
/// A host that hasn't managed to write the lease for this long freezes its world.
pub const SELF_FENCE: Duration = Duration::from_secs(20);
/// A lease whose heartbeat hasn't moved for this long may be taken over.
pub const DEAD: Duration = Duration::from_secs(36);
/// Players waiting to take over go one at a time, this far apart, in the order of the lease's
/// player list.
pub const TURN: Duration = Duration::from_secs(2);
/// What a host whose lease doesn't say its own (a game from before it did) used: its
/// heartbeat, and how long it may stand still. A watcher waits that long for it.
pub const LEGACY_HEARTBEAT: Duration = Duration::from_secs(15);
pub const LEGACY_DEAD: Duration = Duration::from_secs(90);

// The margin the scheme rests on, checked when the crate is built: a host that can't write
// the lease has frozen, with two more heartbeats to spare, before anyone may take over.
const _: () = assert!(SELF_FENCE.as_secs() + 2 * HEARTBEAT.as_secs() < DEAD.as_secs());

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// A host has the world and is getting it ready: nobody can join yet.
    Starting,
    /// The world is open: `code` joins it.
    Hosting,
    /// The host is leaving and has published its last copy; `successor` takes the world next.
    Handoff,
    /// Nobody hosts it. Whoever opens it next does.
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The player's account UUID, as 32 hex digits.
    pub uuid: String,
    pub name: String,
    /// Friends on other networks can reach this player's game: a good next host.
    #[serde(default)]
    pub reachable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub version: u32,
    /// Counts hosts: each game that takes the world raises it by one.
    pub epoch: u64,
    pub state: State,
    pub host: Option<Member>,
    /// The host's device, so the same player on another device is another host.
    pub device: String,
    /// The player (UUID) who takes the world after a handoff; None leaves it to whoever comes.
    pub successor: Option<String>,
    /// Raised with every write: a heartbeat that stands still is a host that stopped.
    pub beat: u64,
    /// Random with every write, so a writer can tell its own write from someone else's.
    pub token: String,
    /// The invite code that joins the host, while hosting.
    pub code: Option<String>,
    /// The code goes through the public tunnel.
    pub tunnel: bool,
    /// Where the public tunnel is reached (one of the code's endpoints): a guest dials it only
    /// when nothing else answers, and still tries for a direct connection over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_at: Option<String>,
    /// The host left the world and uploads its last copy: nobody is in it, though it is still
    /// the host's until the handoff (an older game leaves it out).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closing: bool,
    /// The last backup loaded, and the copy it replaced (what undoing it loads). Carried from
    /// host to host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded: Option<Loaded>,
    /// The host's game's heartbeat, and how long it may stand still before it may be taken
    /// (milliseconds): a watcher waits at least that. A lease without them (an older game's)
    /// is waited for as [`LEGACY_DEAD`] says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beat_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_ms: Option<u64>,
    /// The host's app left the screen (its system may suspend it): nobody can play until it is
    /// back, and the list says so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub away: bool,
    /// Who is in the world, the host first.
    pub players: Vec<Member>,
    /// The newest copy of the save. A copy counts only once it is named here.
    pub generation: Option<GenId>,
}

impl Lease {
    /// Whether this device's own game held the world and stopped without letting it go (it
    /// was closed, or the system ended it). Only a game that isn't hosting it may ask: then
    /// nobody hosts it, and that game takes it straight back, its own progress with it.
    pub fn left_by(&self, me: &str, device: &str) -> bool {
        self.left_here(device) && self.host.as_ref().is_some_and(|h| h.uuid == me)
    }

    /// Whether a game on this device held the world and stopped without letting it go, under
    /// any account (one signed in then, another now). A device runs one game at a time, so a
    /// game here that isn't hosting it knows nobody is: no waiting for a heartbeat that can't
    /// come.
    pub fn left_here(&self, device: &str) -> bool {
        matches!(self.state, State::Hosting | State::Starting)
            && !self.device.is_empty()
            && self.device == device
    }

    /// How many are playing, as the world list shows it.
    pub fn playing(&self) -> usize {
        match self.state {
            State::Hosting | State::Starting if self.closing => 0,
            State::Hosting | State::Starting => self.players.len().max(1),
            State::Handoff | State::Closed => 0,
        }
    }
}

/// A backup that was loaded ([`crate::backups::load`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loaded {
    /// The copy loaded.
    pub backup: GenId,
    /// The copy that was the newest before it: loading it again undoes the load.
    pub replaced: Option<GenId>,
    /// Who asked.
    pub by: String,
    /// The request carried out ([`crate::backups::Request::id`]).
    pub request: String,
    /// When (seconds since 1970, the loading game's clock).
    pub at: i64,
}

/// A lease as the storage had it, with the tag a change must name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub lease: Lease,
    pub etag: Etag,
    /// How long ago it was written, by the storage's clock (None: just written, or a storage
    /// that doesn't say).
    pub age: Option<Duration>,
}

/// Why a game asks for the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// Nobody has it: it is closed, was never opened, or is handed to this player.
    Free,
    /// Its host stopped: the caller watched the heartbeat stand still for [`DEAD`].
    Takeover,
}

/// A world's lease in its storage.
#[derive(Clone)]
pub struct Leases {
    store: Arc<dyn Store>,
    key: String,
}

fn token() -> Result<String> {
    Ok(hex(&random::<12>()?))
}

impl Leases {
    /// `key` is the lease's full key in the bucket (`w/<world>/lease`).
    pub fn new(store: Arc<dyn Store>, key: &str) -> Self {
        Self {
            store,
            key: key.to_owned(),
        }
    }

    pub fn read(&self) -> Result<Option<Seen>> {
        let Some(object) = self.store.get(&self.key)? else {
            return Ok(None);
        };
        let lease: Lease = serde_json::from_slice(&object.data)?;
        if lease.version > VERSION {
            return Err(Error::Format(
                "this world was last hosted by a newer version of the game: update to play it"
                    .into(),
            ));
        }
        Ok(Some(Seen {
            lease,
            etag: object.etag,
            age: object.age,
        }))
    }

    /// Takes the world for `me`. `seen` is the lease as last read (None: there is none yet).
    /// [`Error::Refused`] when the lease doesn't allow it or changed since: read it again.
    pub fn claim(
        &self,
        seen: Option<&Seen>,
        me: &Member,
        device: &str,
        why: Claim,
    ) -> Result<Held> {
        let loaded = seen.and_then(|s| s.lease.loaded.clone());
        let (epoch, generation, condition) = match seen {
            None => (1, None, Condition::Absent),
            Some(seen) => {
                let lease = &seen.lease;
                let free = match lease.state {
                    State::Closed => true,
                    State::Handoff => lease.successor.as_ref().is_none_or(|s| *s == me.uuid),
                    State::Starting | State::Hosting => false,
                };
                if !(free || why == Claim::Takeover) {
                    return Err(Error::Refused);
                }
                (
                    lease.epoch + 1,
                    lease.generation,
                    Condition::Unchanged(seen.etag.clone()),
                )
            }
        };
        let lease = Lease {
            version: VERSION,
            epoch,
            state: State::Starting,
            host: Some(me.clone()),
            device: device.to_owned(),
            successor: None,
            beat: 0,
            token: token()?,
            code: None,
            tunnel: false,
            tunnel_at: None,
            closing: false,
            loaded,
            beat_ms: None,
            dead_ms: None,
            away: false,
            players: vec![me.clone()],
            generation,
        };
        let data = serde_json::to_vec(&lease)?;
        match self.store.put(&self.key, &data, &condition) {
            Ok(etag) => Ok(Held::new(
                self.clone(),
                Seen {
                    lease,
                    etag,
                    age: None,
                },
            )),
            // The write may have landed although its answer was lost.
            Err(error) => match self.read() {
                Ok(Some(found)) if found.lease.token == lease.token => {
                    Ok(Held::new(self.clone(), found))
                }
                _ => Err(error),
            },
        }
    }

    /// The same game picking its own lease up again after a restart, without a new epoch: the
    /// lease still names this player on this device, and nobody took it meanwhile.
    pub fn resume(&self, seen: &Seen, me: &Member, device: &str) -> Result<Held> {
        let lease = &seen.lease;
        let mine = matches!(lease.state, State::Starting | State::Hosting)
            && lease.host.as_ref().is_some_and(|h| h.uuid == me.uuid)
            && lease.device == device;
        if !mine {
            return Err(Error::Refused);
        }
        let mut held = Held::new(self.clone(), seen.clone());
        held.update(|lease| {
            lease.state = State::Starting;
            lease.code = None;
            lease.tunnel = false;
            lease.tunnel_at = None;
            lease.closing = false;
            lease.away = false;
            lease.players = vec![me.clone()];
        })?;
        Ok(held)
    }
}

/// The lease while this game holds it. Every method is one conditional write; after
/// [`Error::Fenced`] the world is someone else's and this value is of no further use.
pub struct Held {
    leases: Leases,
    seen: Seen,
    /// Tokens of writes whose answer never came: any of them may still land.
    unanswered: Vec<String>,
    /// This game's heartbeat and dead time (ms), written with every change from now on.
    pace: Option<(u64, u64)>,
}

impl Held {
    fn new(leases: Leases, seen: Seen) -> Self {
        Self {
            leases,
            seen,
            unanswered: Vec::new(),
            pace: None,
        }
    }

    /// Every write from now on says how this game keeps the lease, for watchers to wait as
    /// long as it needs.
    pub fn set_pace(&mut self, timing: &Timing) {
        let ms = |d: Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
        self.pace = Some((ms(timing.heartbeat), ms(timing.dead)));
    }

    /// The host's app left the screen, or came back.
    pub fn away(&mut self, away: bool) -> Result<()> {
        self.update(|lease| lease.away = away)
    }

    pub fn lease(&self) -> &Lease {
        &self.seen.lease
    }

    /// Changes the lease and raises its heartbeat.
    /// - [`Error::Fenced`]: another game took the world. Stop.
    /// - [`Error::Store`]: the storage wasn't reached, or the answer was lost. The lease may or
    ///   may not hold the change; calling again is safe.
    pub fn update(&mut self, change: impl Fn(&mut Lease)) -> Result<()> {
        // Twice at most: the second time on top of an earlier write of ours that landed late.
        for _ in 0..2 {
            let mut next = self.seen.lease.clone();
            change(&mut next);
            if let Some((beat, dead)) = self.pace {
                next.beat_ms = Some(beat);
                next.dead_ms = Some(dead);
            }
            next.beat += 1;
            next.token = token()?;
            let data = serde_json::to_vec(&next)?;
            let condition = Condition::Unchanged(self.seen.etag.clone());
            let error = match self.leases.store.put(&self.leases.key, &data, &condition) {
                Ok(etag) => {
                    self.seen = Seen {
                        lease: next,
                        etag,
                        age: None,
                    };
                    self.unanswered.clear();
                    return Ok(());
                }
                Err(error @ (Error::Refused | Error::Store(_))) => error,
                Err(other) => return Err(other),
            };
            self.unanswered.push(next.token.clone());
            let found = match self.leases.read() {
                Ok(found) => found,
                // Not known: the lease may be ours still. The caller's own timer decides.
                Err(_) => return Err(Error::Store("the lease couldn't be read back".into())),
            };
            match found {
                Some(found) if found.lease.token == next.token => {
                    // This very write landed; only its answer was lost.
                    self.seen = found;
                    self.unanswered.clear();
                    return Ok(());
                }
                Some(found) if self.unanswered.contains(&found.lease.token) => {
                    // An earlier write of ours landed after we gave up on it: go on from it.
                    self.seen = found;
                    self.unanswered.clear();
                }
                Some(found) if found.etag == self.seen.etag => {
                    // Still the version we hold: the write didn't land, and nobody took over.
                    return Err(match error {
                        Error::Refused => Error::Store("the storage is busy with the lease".into()),
                        other => other,
                    });
                }
                _ => return Err(Error::Fenced),
            }
        }
        Err(Error::Store(
            "the lease kept changing under its own writes".into(),
        ))
    }

    /// The host is alive, with these players (the host first).
    pub fn heartbeat(&mut self, players: &[Member]) -> Result<()> {
        self.update(|lease| lease.players = players.to_vec())
    }

    /// The world is open and `code` joins it; `tunnel` is where its public tunnel is reached,
    /// when it has one.
    pub fn open(&mut self, code: &str, tunnel: Option<&str>) -> Result<()> {
        self.update(|lease| {
            lease.state = State::Hosting;
            lease.code = Some(code.to_owned());
            lease.tunnel = tunnel.is_some();
            lease.tunnel_at = tunnel.map(str::to_owned);
            lease.closing = false;
        })
    }

    /// The host left the world and starts uploading its last copy: nobody can join it, and the
    /// list shows nobody in it, while it stays the host's until [`Self::hand_off`].
    pub fn closing(&mut self, host: &Member) -> Result<()> {
        self.update(|lease| {
            lease.code = None;
            lease.tunnel = false;
            lease.tunnel_at = None;
            lease.closing = true;
            lease.players = vec![host.clone()];
        })
    }

    /// Makes `generation` the world's newest copy. Its files must already be in the storage.
    pub fn commit(&mut self, generation: GenId) -> Result<()> {
        self.update(|lease| lease.generation = Some(generation))
    }

    /// Makes `generation`, a loaded backup's copy, the newest, and records the load.
    pub fn commit_loaded(&mut self, generation: GenId, loaded: Loaded) -> Result<()> {
        self.update(|lease| {
            lease.generation = Some(generation);
            lease.loaded = Some(loaded.clone());
        })
    }

    /// The host leaves, having published `generation`; `successor` (a UUID) hosts next.
    ///
    /// The players stay listed through a handoff: if the successor never comes, the others take
    /// their turns in that order ([`turn`]). Nobody is playing either way ([`Lease::playing`]).
    pub fn hand_off(&mut self, generation: GenId, successor: Option<&str>) -> Result<()> {
        self.update(|lease| {
            lease.state = if successor.is_some() {
                State::Handoff
            } else {
                State::Closed
            };
            lease.generation = Some(generation);
            lease.successor = successor.map(str::to_owned);
            lease.code = None;
            lease.tunnel = false;
            lease.tunnel_at = None;
            if successor.is_none() {
                lease.players.clear();
            }
        })
    }

    /// Gives the world back without a new copy: it couldn't be made ready, or was never opened.
    pub fn release(&mut self) -> Result<()> {
        self.update(|lease| {
            lease.state = State::Closed;
            lease.successor = None;
            lease.code = None;
            lease.tunnel = false;
            lease.tunnel_at = None;
            lease.players.clear();
        })
    }
}

/// The intervals the lease's rules run on. [`Timing::REAL`] is the game's; tests run the same
/// rules faster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub heartbeat: Duration,
    pub self_fence: Duration,
    pub dead: Duration,
    pub turn: Duration,
    /// How long a handoff waits for its successor before the others may take the world.
    pub handoff_wait: Duration,
    /// How often a watcher reads the lease (three times as often while it changes hands).
    pub poll: Duration,
    /// What a lease that doesn't say its host's pace is waited for as ([`LEGACY_HEARTBEAT`],
    /// [`LEGACY_DEAD`]).
    pub legacy_heartbeat: Duration,
    pub legacy_dead: Duration,
}

impl Timing {
    pub const REAL: Self = Self {
        heartbeat: HEARTBEAT,
        self_fence: SELF_FENCE,
        dead: DEAD,
        turn: TURN,
        handoff_wait: Duration::from_secs(15),
        poll: Duration::from_secs(3),
        legacy_heartbeat: LEGACY_HEARTBEAT,
        legacy_dead: LEGACY_DEAD,
    };

    /// Whether the rules hold together: a host that can't write freezes, with two heartbeats
    /// to spare, before anyone may take over, and watchers look often enough to see it.
    pub fn consistent(&self) -> bool {
        self.self_fence + self.heartbeat * 2 < self.dead
            && self.poll < self.heartbeat
            && self.handoff_wait < self.dead
    }

    /// Whether a host whose lease stood still for `quiet` has missed its heartbeats (two, and
    /// two reads late): others may not take the world yet, but it is shown as not answering.
    pub fn silent(&self, lease: &Lease, quiet: Duration) -> bool {
        quiet >= self.heartbeat_of(lease) * 2 + self.poll * 2
    }

    /// How often `lease`'s host writes it: what it says (at least this game's own heartbeat),
    /// or an older game's.
    pub fn heartbeat_of(&self, lease: &Lease) -> Duration {
        lease
            .beat_ms
            .map_or(self.legacy_heartbeat, Duration::from_millis)
            .max(self.heartbeat)
    }

    /// How long `lease`'s host may stand still before it may be taken: what its lease says
    /// (never less than this game's own), or an older game's [`LEGACY_DEAD`].
    pub fn dead_for(&self, lease: &Lease) -> Duration {
        lease
            .dead_ms
            .map_or(self.legacy_dead, Duration::from_millis)
            .max(self.dead)
    }

    /// How long the watcher whose turn is `turn` still waits before it may take a lease that
    /// stood still for `quiet`.
    pub fn left_to_wait(&self, lease: &Lease, quiet: Duration, turn: usize) -> Duration {
        let turn = u32::try_from(turn).unwrap_or(u32::MAX);
        self.dead_for(lease)
            .saturating_add(self.turn.saturating_mul(turn))
            .saturating_sub(quiet)
    }

    /// Whether a lease quiet for `quiet` may be taken, after `base`, by the watcher whose turn
    /// is `turn`.
    pub fn after(&self, base: Duration, quiet: Duration, turn: usize) -> bool {
        let turn = u32::try_from(turn).unwrap_or(u32::MAX);
        quiet >= base.saturating_add(self.turn.saturating_mul(turn))
    }
}

/// A watcher's view of a lease it doesn't hold: how long its heartbeat has stood still, on the
/// watcher's own timer.
#[derive(Clone, Debug)]
pub struct Watch {
    mark: Option<(u64, u64, String)>,
    since: Instant,
    /// When the lease was last read ([`Self::observe_seen`]).
    looked: Option<Instant>,
    /// The storage once called a write older than it could be (one this watcher saw happen
    /// since its last look): its ages aren't used from then on.
    doubted: bool,
}

/// What the storage's own age of a lease is shaved by before it counts: whole seconds, and
/// the storage's front and back ends' clocks, are never held against a live host.
pub const AGE_MARGIN: Duration = Duration::from_secs(10);

impl Watch {
    pub fn new(now: Instant) -> Self {
        Self {
            mark: None,
            since: now,
            looked: None,
            doubted: false,
        }
    }

    /// As [`Self::observe`], and at least as old as the storage says the lease is: a host that
    /// stopped long before this watcher first looked is seen to have stopped long ago, not
    /// just now. Both are the storage's clock or this device's alone, never one against the
    /// other.
    pub fn observe_seen(&mut self, seen: &Seen, now: Instant) -> Duration {
        let lease = &seen.lease;
        let changed = self.mark.as_ref().is_some_and(|(epoch, beat, token)| {
            (*epoch, *beat, token.as_str()) != (lease.epoch, lease.beat, lease.token.as_str())
        });
        let last_look = self.looked.replace(now);
        let here = self.observe(lease, now);
        // A write seen to happen since the last look is no older than that look: a storage that
        // says otherwise isn't believed again.
        if changed
            && let (Some(age), Some(then)) = (seen.age, last_look)
            && age > now.saturating_duration_since(then) + AGE_MARGIN
        {
            if !self.doubted {
                log::warn!(
                    "Shared worlds: the storage's clock called a new write {} s old: only this device's is used",
                    age.as_secs()
                );
            }
            self.doubted = true;
        }
        if self.doubted {
            return here;
        }
        here.max(seen.age.map_or(Duration::ZERO, |a| a.saturating_sub(AGE_MARGIN)))
    }

    /// Notes the lease as read at `now`, and returns how long it has been the same write.
    pub fn observe(&mut self, lease: &Lease, now: Instant) -> Duration {
        let mark = (lease.epoch, lease.beat, lease.token.clone());
        if self.mark.as_ref() != Some(&mark) {
            self.mark = Some(mark);
            self.since = now;
        }
        now.saturating_duration_since(self.since)
    }

    /// Whether a lease quiet for `quiet` may be taken by the watcher whose place in the lease's
    /// player list (the host aside) is `turn`: the first at [`DEAD`], each next one [`TURN`] later.
    pub fn may_take(quiet: Duration, turn: usize) -> bool {
        Timing::REAL.after(DEAD, quiet, turn)
    }
}

/// A watcher's place in line to take a world whose host stopped: its position in the lease's
/// player list with the host left out. Someone not in the list goes after everyone in it.
pub fn turn(lease: &Lease, me: &str) -> usize {
    let host = lease.host.as_ref().map(|h| h.uuid.as_str());
    let waiting: Vec<&str> = lease
        .players
        .iter()
        .map(|p| p.uuid.as_str())
        .filter(|uuid| Some(*uuid) != host)
        .collect();
    waiting
        .iter()
        .position(|uuid| *uuid == me)
        .unwrap_or(waiting.len())
}

/// The host's own timer: when did a write of the lease last succeed. Give it the time the
/// write was *sent*, so the count starts no later than the storage's does.
#[derive(Debug)]
pub struct Pulse {
    last: Instant,
}

impl Pulse {
    pub fn new(sent: Instant) -> Self {
        Self { last: sent }
    }

    pub fn succeeded(&mut self, sent: Instant) {
        self.last = self.last.max(sent);
    }

    /// The host must stop changing the world until a write succeeds again.
    pub fn must_freeze(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last) >= SELF_FENCE
    }

    /// Time for the next heartbeat.
    pub fn due(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last) >= HEARTBEAT
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_closing_lease_has_nobody_playing_and_opening_again_clears_it() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store, KEY);
        let mut held = leases
            .claim(None, &member(1), "phone", Claim::Free)
            .unwrap();
        held.open("C", Some("203.0.113.9:4000")).unwrap();
        held.heartbeat(&[member(1), member(2)]).unwrap();
        assert_eq!(held.lease().playing(), 2);
        held.closing(&member(1)).unwrap();
        let seen = leases.read().unwrap().unwrap().lease;
        assert!(seen.closing && seen.code.is_none() && !seen.tunnel && seen.tunnel_at.is_none());
        assert_eq!(seen.playing(), 0);
        assert_eq!(seen.state, State::Hosting);
        held.open("D", None).unwrap();
        assert!(!held.lease().closing);
        assert_eq!(held.lease().playing(), 1);
        // A closing lease's text names the field; an older game ignores it.
        let text = serde_json::to_string(&seen).unwrap();
        assert!(text.contains(r#""closing":true"#), "{text}");
    }

    #[test]
    fn a_lease_without_the_tunnel_address_reads_and_writes_as_before() {
        // As a game from before `tunnel_at` writes it.
        let old = r#"{"version":1,"epoch":3,"state":"hosting","host":null,"device":"d","successor":null,"beat":7,"token":"t","code":"C","tunnel":true,"players":[],"generation":null}"#;
        let lease: Lease = serde_json::from_str(old).unwrap();
        assert!(lease.tunnel);
        assert_eq!(lease.tunnel_at, None);
        // Written back, it has no new field for an older game to stumble on.
        let back = serde_json::to_string(&lease).unwrap();
        assert!(
            !back.contains("tunnel_at") && !back.contains("closing"),
            "{back}"
        );
        assert!(!lease.closing);
        let with = Lease {
            tunnel_at: Some("203.0.113.9:4000".into()),
            ..lease
        };
        let text = serde_json::to_string(&with).unwrap();
        assert_eq!(serde_json::from_str::<Lease>(&text).unwrap(), with);
    }

    use super::*;
    use crate::store::MemoryStore;
    use std::sync::atomic::Ordering;

    const KEY: &str = "w/test/lease";

    fn member(n: u32) -> Member {
        Member {
            uuid: format!("{n:032x}"),
            name: format!("Player{n}"),
            reachable: false,
        }
    }

    fn generation(epoch: u64, seq: u32) -> GenId {
        GenId {
            epoch,
            seq,
            day: 20261113,
        }
    }

    #[test]
    fn the_first_to_open_a_world_hosts_it() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        assert!(leases.read().unwrap().is_none());
        let mut held = leases
            .claim(None, &member(1), "phone", Claim::Free)
            .unwrap();
        assert_eq!(
            (held.lease().epoch, held.lease().state),
            (1, State::Starting)
        );
        // A second game that also saw no lease is refused.
        assert!(matches!(
            leases.claim(None, &member(2), "pc", Claim::Free),
            Err(Error::Refused)
        ));
        held.open("CODE-1", None).unwrap();
        held.heartbeat(&[member(1), member(2)]).unwrap();
        let seen = leases.read().unwrap().unwrap();
        assert_eq!(seen.lease.state, State::Hosting);
        assert_eq!(seen.lease.code.as_deref(), Some("CODE-1"));
        assert_eq!(seen.lease.playing(), 2);
        assert_eq!(seen.lease.beat, 2);
        // While it is hosted nobody else may claim it as free.
        assert!(matches!(
            leases.claim(Some(&seen), &member(2), "pc", Claim::Free),
            Err(Error::Refused)
        ));
    }

    #[test]
    fn a_closed_world_goes_to_one_of_two_who_open_it_together() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut first = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        first.commit(generation(1, 1)).unwrap();
        first.hand_off(generation(1, 2), None).unwrap();
        let seen = leases.read().unwrap().unwrap();
        assert_eq!((seen.lease.state, seen.lease.playing()), (State::Closed, 0));
        // Both read the same closed lease.
        let second = leases
            .claim(Some(&seen), &member(2), "b", Claim::Free)
            .unwrap();
        assert!(matches!(
            leases.claim(Some(&seen), &member(3), "c", Claim::Free),
            Err(Error::Refused)
        ));
        assert_eq!(second.lease().epoch, 2);
        assert_eq!(
            second.lease().generation,
            Some(generation(1, 2)),
            "the newest copy carries over"
        );
    }

    #[test]
    fn a_handoff_is_for_its_successor() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut host = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        host.open("C", None).unwrap();
        host.hand_off(generation(1, 1), Some(&member(2).uuid))
            .unwrap();
        let seen = leases.read().unwrap().unwrap();
        assert_eq!(seen.lease.state, State::Handoff);
        assert!(matches!(
            leases.claim(Some(&seen), &member(3), "c", Claim::Free),
            Err(Error::Refused)
        ));
        let next = leases
            .claim(Some(&seen), &member(2), "b", Claim::Free)
            .unwrap();
        assert_eq!(next.lease().epoch, 2);
        // The old host can't write any more.
        assert!(matches!(host.heartbeat(&[member(1)]), Err(Error::Fenced)));
    }

    #[test]
    fn a_host_that_was_replaced_is_fenced() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut old = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        old.open("C", None).unwrap();
        let seen = leases.read().unwrap().unwrap();
        let mut new = leases
            .claim(Some(&seen), &member(2), "b", Claim::Takeover)
            .unwrap();
        assert!(matches!(old.commit(generation(1, 9)), Err(Error::Fenced)));
        assert!(matches!(old.heartbeat(&[member(1)]), Err(Error::Fenced)));
        new.commit(generation(2, 1)).unwrap();
        let lease = leases.read().unwrap().unwrap().lease;
        assert_eq!(
            lease.generation,
            Some(generation(2, 1)),
            "the old host's copy never counts"
        );
        assert_eq!(lease.host, Some(member(2)));
    }

    #[test]
    fn a_write_whose_answer_was_lost_still_counts() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        store.fail_after_write.store(1, Ordering::SeqCst);
        let mut held = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        assert_eq!(held.lease().epoch, 1);
        store.fail_after_write.store(1, Ordering::SeqCst);
        held.commit(generation(1, 1)).unwrap();
        held.heartbeat(&[member(1)]).unwrap();
        let lease = leases.read().unwrap().unwrap().lease;
        assert_eq!((lease.beat, lease.generation), (2, Some(generation(1, 1))));
    }

    #[test]
    fn an_unreachable_storage_is_not_a_lost_world() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut held = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        // The write fails, and so does reading it back.
        store.fail_before.store(2, Ordering::SeqCst);
        assert!(matches!(held.heartbeat(&[member(1)]), Err(Error::Store(_))));
        // The write fails; the lease reads back unchanged.
        store.fail_before.store(1, Ordering::SeqCst);
        assert!(matches!(held.heartbeat(&[member(1)]), Err(Error::Store(_))));
        // And then it works again, from where it was.
        held.heartbeat(&[member(1)]).unwrap();
        assert_eq!(leases.read().unwrap().unwrap().lease.beat, 1);
    }

    #[test]
    fn a_game_picks_its_own_lease_up_after_a_restart() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut held = leases
            .claim(None, &member(1), "phone", Claim::Free)
            .unwrap();
        held.open("C", Some("203.0.113.9:4000")).unwrap();
        drop(held);
        let seen = leases.read().unwrap().unwrap();
        assert!(seen.lease.tunnel);
        assert_eq!(seen.lease.tunnel_at.as_deref(), Some("203.0.113.9:4000"));
        assert!(matches!(
            leases.resume(&seen, &member(1), "pc"),
            Err(Error::Refused)
        ));
        assert!(matches!(
            leases.resume(&seen, &member(2), "phone"),
            Err(Error::Refused)
        ));
        let again = leases.resume(&seen, &member(1), "phone").unwrap();
        assert_eq!(
            (again.lease().epoch, again.lease().state),
            (1, State::Starting)
        );
        assert_eq!(again.lease().code, None);
        assert!(!again.lease().tunnel);
        assert_eq!(again.lease().tunnel_at, None);
        // The lease read before the restart is stale now.
        assert!(matches!(
            leases.resume(&seen, &member(1), "phone"),
            Err(Error::Fenced)
        ));
    }

    #[test]
    fn a_lease_from_a_newer_format_is_left_alone() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let held = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        let mut lease = held.lease().clone();
        lease.version = VERSION + 1;
        store
            .put(
                KEY,
                &serde_json::to_vec(&lease).unwrap(),
                &Condition::Always,
            )
            .unwrap();
        assert!(matches!(leases.read(), Err(Error::Format(_))));
        store.put(KEY, b"not json", &Condition::Always).unwrap();
        assert!(matches!(leases.read(), Err(Error::Format(_))));
    }

    #[test]
    fn a_heartbeat_that_stands_still_is_a_host_that_stopped() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut held = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        held.heartbeat(&[member(1), member(2), member(3)]).unwrap();
        let start = Instant::now();
        let mut watch = Watch::new(start);
        let lease = leases.read().unwrap().unwrap().lease;
        assert_eq!(watch.observe(&lease, start), Duration::ZERO);
        assert_eq!(
            watch.observe(&lease, start + Duration::from_secs(60)),
            Duration::from_secs(60)
        );
        // The host writes again: the count starts over.
        held.heartbeat(&[member(1), member(2), member(3)]).unwrap();
        let lease = leases.read().unwrap().unwrap().lease;
        assert_eq!(
            watch.observe(&lease, start + Duration::from_secs(70)),
            Duration::ZERO
        );
        let quiet = watch.observe(&lease, start + Duration::from_secs(70) + DEAD);
        assert_eq!(quiet, DEAD);
        // The players take their turns, the host aside.
        assert_eq!(turn(&lease, &member(2).uuid), 0);
        assert_eq!(turn(&lease, &member(3).uuid), 1);
        assert_eq!(turn(&lease, &member(9).uuid), 2);
        assert!(Watch::may_take(quiet, 0));
        assert!(!Watch::may_take(quiet, 1));
        assert!(Watch::may_take(quiet + TURN, 1));
        assert!(!Watch::may_take(DEAD - Duration::from_millis(1), 0));
    }

    /// The storage's age counts (less its margin) from the first look; a storage that calls a
    /// write this watcher saw happen older than it can be isn't believed again.
    #[test]
    fn the_storages_age_counts_unless_it_is_seen_wrong() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut host = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        let start = Instant::now();
        let mut watch = Watch::new(start);
        let mut seen = leases.read().unwrap().unwrap();
        seen.age = Some(Duration::from_secs(100));
        assert_eq!(
            watch.observe_seen(&seen, start),
            Duration::from_secs(100) - AGE_MARGIN
        );
        // Within the margin, nothing: a live host is never aged by a few seconds' rounding.
        let mut near = Watch::new(start);
        seen.age = Some(AGE_MARGIN);
        assert_eq!(near.observe_seen(&seen, start), Duration::ZERO);
        // A new write a second after the last look, called 100 s old: doubted, for good.
        host.heartbeat(&[member(1)]).unwrap();
        let mut fresh = leases.read().unwrap().unwrap();
        fresh.age = Some(Duration::from_secs(100));
        let later = start + Duration::from_secs(1);
        assert_eq!(watch.observe_seen(&fresh, later), Duration::ZERO);
        let again = later + Duration::from_secs(5);
        assert_eq!(
            watch.observe_seen(&fresh, again),
            Duration::from_secs(5)
        );
        // Believed, a new write called as old as the time since the last look allows.
        let mut fair = Watch::new(start);
        fair.observe_seen(&seen, start);
        host.heartbeat(&[member(1)]).unwrap();
        let mut next = leases.read().unwrap().unwrap();
        next.age = Some(Duration::from_secs(12));
        let soon = start + Duration::from_secs(3);
        assert_eq!(fair.observe_seen(&next, soon), Duration::from_secs(2));
    }

    #[test]
    fn the_real_timing_holds_together() {
        assert!(Timing::REAL.consistent());
        let mut fast = Timing::REAL;
        fast.self_fence = fast.dead;
        assert!(!fast.consistent());
    }

    #[test]
    fn a_handoff_keeps_the_players_in_line_and_a_release_frees_the_world() {
        let store = Arc::new(MemoryStore::new());
        let leases = Leases::new(store.clone(), KEY);
        let mut host = leases.claim(None, &member(1), "a", Claim::Free).unwrap();
        host.heartbeat(&[member(1), member(2), member(3)]).unwrap();
        host.hand_off(generation(1, 1), Some(&member(2).uuid))
            .unwrap();
        let lease = leases.read().unwrap().unwrap().lease;
        assert_eq!(lease.playing(), 0);
        assert_eq!(turn(&lease, &member(3).uuid), 1);
        let seen = leases.read().unwrap().unwrap();
        let mut next = leases
            .claim(Some(&seen), &member(2), "b", Claim::Free)
            .unwrap();
        next.release().unwrap();
        let lease = leases.read().unwrap().unwrap().lease;
        assert_eq!(
            (lease.state, lease.playing(), lease.generation),
            (State::Closed, 0, Some(generation(1, 1)))
        );
        assert!(lease.players.is_empty());
    }

    #[test]
    fn a_host_freezes_before_anyone_may_take_over() {
        let start = Instant::now();
        let mut pulse = Pulse::new(start);
        let ms = Duration::from_millis(1);
        assert!(!pulse.due(start + HEARTBEAT - ms));
        assert!(pulse.due(start + HEARTBEAT));
        assert!(!pulse.must_freeze(start + SELF_FENCE - ms));
        assert!(pulse.must_freeze(start + SELF_FENCE));
        let later = start + SELF_FENCE * 2;
        pulse.succeeded(later);
        assert!(!pulse.must_freeze(later + SELF_FENCE - ms));
        // An answer that comes back out of order doesn't move the timer backwards.
        pulse.succeeded(start + HEARTBEAT);
        assert!(!pulse.must_freeze(later + SELF_FENCE - ms));
    }

    /// A small deterministic generator, so a failing run can be repeated.
    struct Random(u64);
    impl Random {
        fn next(&mut self, below: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % below
        }
    }

    /// Many games on one lease, in a random order, with the storage failing at random: at no
    /// point do two of them write under one epoch, the epoch never goes back, and neither does
    /// the newest copy.
    #[test]
    fn many_games_never_share_an_epoch() {
        for seed in 1..=40u64 {
            let store = Arc::new(MemoryStore::new());
            let leases = Leases::new(store.clone(), KEY);
            let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut games: Vec<Option<Held>> = (0..5).map(|_| None).collect();
            let mut owner: std::collections::HashMap<u64, usize> = Default::default();
            let mut sequence = 0u32;
            let (mut last_epoch, mut last_generation) = (0u64, None::<GenId>);
            for _ in 0..600 {
                let who = random.next(5) as usize;
                match random.next(10) {
                    0 => store.fail_before.store(1, Ordering::SeqCst),
                    1 => store.fail_after_write.store(1, Ordering::SeqCst),
                    _ => {}
                }
                let me = member(who as u32);
                if let Some(mut held) = games[who].take() {
                    let epoch = held.lease().epoch;
                    let result = match random.next(4) {
                        0 => {
                            sequence += 1;
                            held.commit(generation(epoch, sequence))
                        }
                        1 => {
                            sequence += 1;
                            held.hand_off(generation(epoch, sequence), None)
                        }
                        _ => held.heartbeat(std::slice::from_ref(&me)),
                    };
                    match result {
                        Ok(()) => {
                            assert_eq!(*owner.entry(epoch).or_insert(who), who, "seed {seed}");
                            if held.lease().state != State::Closed {
                                games[who] = Some(held);
                            }
                        }
                        Err(Error::Fenced) => {}
                        Err(Error::Store(_)) => games[who] = Some(held),
                        Err(other) => panic!("seed {seed}: {other}"),
                    }
                } else if let Ok(seen) = leases.read() {
                    let why = if random.next(6) == 0 {
                        Claim::Takeover
                    } else {
                        Claim::Free
                    };
                    match leases.claim(seen.as_ref(), &me, &format!("device{who}"), why) {
                        Ok(held) => {
                            let epoch = held.lease().epoch;
                            assert_eq!(*owner.entry(epoch).or_insert(who), who, "seed {seed}");
                            games[who] = Some(held);
                        }
                        Err(Error::Refused | Error::Store(_)) => {}
                        Err(other) => panic!("seed {seed}: {other}"),
                    }
                }
                check(&store, &leases, &mut last_epoch, &mut last_generation, seed);
            }
        }
    }

    fn check(
        store: &MemoryStore,
        leases: &Leases,
        last_epoch: &mut u64,
        last_generation: &mut Option<GenId>,
        seed: u64,
    ) {
        // The check itself must not be failed by the switches.
        store.fail_before.store(0, Ordering::SeqCst);
        let Some(seen) = leases.read().unwrap() else {
            return;
        };
        assert!(
            seen.lease.epoch >= *last_epoch,
            "seed {seed}: the epoch went back"
        );
        assert!(
            seen.lease.generation >= *last_generation,
            "seed {seed}: the newest copy went back"
        );
        *last_epoch = seen.lease.epoch;
        *last_generation = seen.lease.generation;
    }
}
