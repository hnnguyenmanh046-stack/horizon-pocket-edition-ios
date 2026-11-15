//! Backups: the older copies of a world its storage keeps ([`crate::retention`] decides which),
//! how any member asks to load one, how the world's owner (the player who shared it) says yes
//! or no, and the load itself.
//!
//! Loading copies nothing: it writes a new copy whose list of files points into the blocks the
//! backup already has, and makes that the world's newest, as any copy is made newest (by its
//! lease). The copy it replaced stays kept, so a load is undone by loading that one.
//!
//! In the storage, beside the world's lease and copies:
//!
//! | Object | What it is |
//! |---|---|
//! | `world.json` | Who shared the world, and how often a backup is kept ([`WorldMeta`]) |
//! | `load.json` | The latest request to load a backup ([`Request`]), and what became of it |
//! | `stamp/<epoch>-<seq>-<time>-<kind>-<name>` | Empty: when a copy was made, by whom, and why |
use crate::{
    Error, Result,
    lease::{Held, Loaded, Member},
    manifest::{GenId, Manifest, VERSION},
    store::{Condition, Etag},
    sync::Remote,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const META: &str = "world.json";
pub const REQUEST: &str = "load.json";
const STAMPS: &str = "stamp/";

/// Seconds since 1970, on this device's clock: only ever shown to people, never compared
/// between devices.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Who shared the world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMeta {
    pub version: u32,
    pub owner: Member,
    pub created: i64,
    /// Minutes between the backups its host keeps while it is played (one of [`EVERY`]); left
    /// out, [`EVERY_DEFAULT`]. The owner's to choose ([`set_every`]); an older game leaves it be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<u32>,
}

/// The minutes a world's owner may choose between backups. Each backup is also the most a
/// host whose phone dies can lose, and a short pause of the world while it is saved.
pub const EVERY: [u32; 6] = [2, 5, 10, 15, 30, 60];
/// As the game's own autosave.
pub const EVERY_DEFAULT: u32 = 5;

/// The minutes between backups a record says: one of [`EVERY`] (a value no game offers is taken
/// as the nearest one that is).
pub fn every_of(meta: Option<&WorldMeta>) -> u32 {
    match meta.and_then(|m| m.every) {
        None => EVERY_DEFAULT,
        Some(asked) => EVERY
            .into_iter()
            .min_by_key(|m| m.abs_diff(asked))
            .unwrap_or(EVERY_DEFAULT),
    }
}

/// The world's record and its tag, if it has one.
pub fn meta(remote: Remote<'_>) -> Result<Option<(WorldMeta, Etag)>> {
    let Some(object) = remote.store.get(&remote.key(META))? else {
        return Ok(None);
    };
    let meta = serde_json::from_slice(&object.data)
        .map_err(|e| Error::Format(format!("the world's record doesn't read: {e}")))?;
    Ok(Some((meta, object.etag)))
}

/// The minutes between backups the world's owner chose.
pub fn every(remote: Remote<'_>) -> Result<u32> {
    Ok(every_of(meta(remote)?.as_ref().map(|(m, _)| m)))
}

/// The owner (`me`) chooses the minutes between backups, one of [`EVERY`]. Refused to anyone
/// else, and for a world whose owner isn't recorded yet (its sharer's game records it first).
pub fn set_every(remote: Remote<'_>, me: &Member, minutes: u32) -> Result<()> {
    if !EVERY.contains(&minutes) {
        return Err(Error::Format(format!("{minutes} minutes isn't one of {EVERY:?}")));
    }
    // A write between the read and this one (another field, or the same choice twice): again.
    for _ in 0..5 {
        let Some((mut meta, etag)) = meta(remote)? else {
            return Err(Error::Refused);
        };
        if meta.owner.uuid != me.uuid {
            return Err(Error::Refused);
        }
        meta.every = Some(minutes);
        match remote.store.put(
            &remote.key(META),
            &serde_json::to_vec(&meta)?,
            &Condition::Unchanged(etag),
        ) {
            Ok(_) => return Ok(()),
            Err(Error::Refused) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(Error::Store("the world's record kept changing".into()))
}

/// Who says yes to loading a backup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    Known(Member),
    /// A world shared before owners were kept: the name its first copy was made by.
    Named(String),
    /// Nor that: the first copy is gone. The one who shared it may claim it.
    Unknown,
}

impl Owner {
    pub fn is(&self, me: &Member) -> bool {
        match self {
            Self::Known(owner) => owner.uuid == me.uuid,
            Self::Named(name) => *name == me.name,
            Self::Unknown => false,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Known(owner) => Some(&owner.name),
            Self::Named(name) => Some(name),
            Self::Unknown => None,
        }
    }
}

/// The world's owner, as the storage records it.
pub fn owner(remote: Remote<'_>) -> Result<Owner> {
    if let Some((meta, _)) = meta(remote)? {
        return Ok(Owner::Known(meta.owner));
    }
    // Shared before owners were kept: whoever made its first copy shared it.
    let first = crate::retention::generations(remote)?
        .into_iter()
        .find(|id| id.epoch == 1 && id.seq == 1);
    match first {
        Some(id) => Ok(Owner::Named(remote.manifest(id)?.by)),
        None => Ok(Owner::Unknown),
    }
}

/// Records `me` as the world's owner, unless someone is already: true when this did. Called
/// when a world is shared, and by its sharer for a world shared before owners were kept.
pub fn record_owner(remote: Remote<'_>, me: &Member) -> Result<bool> {
    let meta = WorldMeta {
        version: 1,
        owner: me.clone(),
        created: now(),
        every: None,
    };
    match remote.store.put(
        &remote.key(META),
        &serde_json::to_vec(&meta)?,
        &Condition::Absent,
    ) {
        Ok(_) => Ok(true),
        Err(Error::Refused) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Why a copy was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The world's first copy, when it was shared.
    First,
    /// While its host played (every few minutes, and when the app left the screen).
    Checkpoint,
    /// When its host left.
    Left,
    /// A backup loaded: this copy is that one.
    Loaded(GenId),
}

impl Kind {
    fn code(self) -> String {
        match self {
            Self::First => "f".into(),
            Self::Checkpoint => "c".into(),
            Self::Left => "l".into(),
            Self::Loaded(id) => format!("r{}.{}.{}", id.epoch, id.seq, id.day),
        }
    }

    fn parse(code: &str) -> Option<Self> {
        Some(match code {
            "f" => Self::First,
            "c" => Self::Checkpoint,
            "l" => Self::Left,
            _ => {
                let mut parts = code.strip_prefix('r')?.split('.');
                let id = GenId {
                    epoch: parts.next()?.parse().ok()?,
                    seq: parts.next()?.parse().ok()?,
                    day: parts.next()?.parse().ok()?,
                };
                if parts.next().is_some() {
                    return None;
                }
                Self::Loaded(id)
            }
        })
    }
}

/// A name as a stamp's key holds it: letters, digits and `_` (as Minecraft names are), at most
/// 16.
fn key_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(16)
        .collect();
    if kept.is_empty() { "_".into() } else { kept }
}

fn stamp_key(id: GenId, kind: Kind, by: &str, at: i64) -> String {
    format!(
        "{STAMPS}{:010}-{:06}-{}-{}-{}",
        id.epoch,
        id.seq,
        at.max(0),
        kind.code(),
        key_name(by)
    )
}

/// ((epoch, seq), time, kind, by) of a stamp's key under the world's prefix.
fn parse_stamp(key: &str) -> Option<((u64, u32), i64, Kind, String)> {
    let mut parts = key.strip_prefix(STAMPS)?.splitn(5, '-');
    let epoch = parts.next()?.parse().ok()?;
    let seq = parts.next()?.parse().ok()?;
    let at = parts.next()?.parse().ok()?;
    let kind = Kind::parse(parts.next()?)?;
    let by = parts.next()?.to_owned();
    Some(((epoch, seq), at, kind, by))
}

/// Notes when `id` was made, by whom and why. Only for showing backups: a copy without one is
/// shown with its day alone.
pub fn stamp(remote: Remote<'_>, id: GenId, kind: Kind, by: &str, at: i64) -> Result<()> {
    remote
        .store
        .put(
            &remote.key(&stamp_key(id, kind, by, at)),
            &[],
            &Condition::Always,
        )
        .map(|_| ())
}

/// The stamps of `id`, for removing them with it.
pub(crate) fn stamp_keys(listed: &[String], id: GenId) -> Vec<String> {
    listed
        .iter()
        .filter(|k| parse_stamp(k).is_some_and(|(of, ..)| of == (id.epoch, id.seq)))
        .cloned()
        .collect()
}

/// One copy the storage keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backup {
    pub id: GenId,
    /// When it was made (seconds since 1970, on its maker's clock), if it says.
    pub at: Option<i64>,
    pub by: Option<String>,
    pub kind: Option<Kind>,
}

/// Every copy the storage keeps, the newest first. A copy without a stamp (made before stamps
/// were kept) has the time its manifest was written, as the storage lists it.
pub fn list(remote: Remote<'_>) -> Result<Vec<Backup>> {
    let prefix = remote.key("");
    let written: BTreeMap<GenId, i64> = remote
        .store
        .list(&remote.key("gen/"))?
        .into_iter()
        .filter_map(|o| {
            let id = GenId::from_manifest_key(o.key.strip_prefix(&prefix)?)?;
            Some((id, o.modified?))
        })
        .collect();
    let mut stamps: BTreeMap<(u64, u32), (i64, Kind, String)> = BTreeMap::new();
    for object in remote.store.list(&remote.key(STAMPS))? {
        if let Some((of, at, kind, by)) = object.key.strip_prefix(&prefix).and_then(parse_stamp) {
            stamps.insert(of, (at, kind, by));
        }
    }
    let mut out: Vec<Backup> = crate::retention::generations(remote)?
        .into_iter()
        .map(|id| {
            let stamp = stamps.get(&(id.epoch, id.seq));
            Backup {
                id,
                at: stamp.map(|s| s.0).or_else(|| written.get(&id).copied()),
                kind: stamp.map(|s| s.1),
                by: stamp.map(|s| s.2.clone()),
            }
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.id));
    Ok(out)
}

/// How much the bucket holds: (all of it, this world's part).
pub fn usage(remote: Remote<'_>) -> Result<(u64, u64)> {
    let mine = remote.key("");
    let mut all = 0u64;
    let mut world = 0u64;
    for object in remote.store.list("")? {
        all += object.size;
        if object.key.starts_with(&mine) {
            world += object.size;
        }
    }
    Ok((all, world))
}

/// Where a request to load a backup stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Waiting for the owner.
    Asked,
    /// The owner said yes: the next game that takes the world loads it (at once when nobody is
    /// playing; the host's game closes the world for it otherwise).
    Approved,
    Declined,
    /// Withdrawn by the one who asked, or the owner, before it was loaded.
    Withdrawn,
    Done,
    /// It couldn't be loaded (`note` says why).
    Failed,
}

impl State {
    /// Still to be decided or carried out: no other request may be made meanwhile.
    pub fn open(self) -> bool {
        matches!(self, Self::Asked | Self::Approved)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    /// Random: tells this request from any other, and the lease names the one it carried out.
    pub id: String,
    pub backup: GenId,
    pub by: Member,
    pub at: i64,
    pub state: State,
    /// It takes back the last load (its backup is the copy that load replaced).
    #[serde(default)]
    pub undo: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A request as the storage has it, with the tag a change must name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub request: Request,
    pub etag: Etag,
}

pub fn request(remote: Remote<'_>) -> Result<Option<Seen>> {
    let Some(object) = remote.store.get(&remote.key(REQUEST))? else {
        return Ok(None);
    };
    let request = serde_json::from_slice(&object.data)
        .map_err(|e| Error::Format(format!("the load request doesn't read: {e}")))?;
    Ok(Some(Seen {
        request,
        etag: object.etag,
    }))
}

fn write(remote: Remote<'_>, request: &Request, condition: &Condition) -> Result<Etag> {
    remote.store.put(
        &remote.key(REQUEST),
        &serde_json::to_vec(request)?,
        condition,
    )
}

/// `me` asks to load `backup` (one of [`list`]'s); from the owner it is approved at once.
/// [`Error::Busy`] while another request is open; [`Error::Refused`] when one was made at the
/// same moment (read it, and ask again if it still matters).
pub fn ask(
    remote: Remote<'_>,
    backup: GenId,
    me: &Member,
    owner: &Owner,
    undo: bool,
) -> Result<Request> {
    let condition = match request(remote)? {
        Some(seen) if seen.request.state.open() => return Err(Error::Busy),
        Some(seen) => Condition::Unchanged(seen.etag),
        None => Condition::Absent,
    };
    let mine = owner.is(me);
    let request = Request {
        version: 1,
        id: crate::codec::hex(&crate::codec::random::<8>()?),
        backup,
        by: me.clone(),
        at: now(),
        state: if mine { State::Approved } else { State::Asked },
        undo,
        decided_by: mine.then(|| me.name.clone()),
        note: None,
    };
    write(remote, &request, &condition)?;
    Ok(request)
}

/// The owner's answer to a request that waits for it. [`Error::Refused`] when it changed since
/// it was read, or `me` isn't the owner.
pub fn decide(
    remote: Remote<'_>,
    seen: &Seen,
    me: &Member,
    owner: &Owner,
    approve: bool,
) -> Result<Request> {
    if !owner.is(me) || seen.request.state != State::Asked {
        return Err(Error::Refused);
    }
    let mut request = seen.request.clone();
    request.state = if approve {
        State::Approved
    } else {
        State::Declined
    };
    request.decided_by = Some(me.name.clone());
    write(remote, &request, &Condition::Unchanged(seen.etag.clone()))?;
    Ok(request)
}

/// Takes back an open request: its asker's or the owner's to do.
pub fn withdraw(remote: Remote<'_>, seen: &Seen, me: &Member, owner: &Owner) -> Result<()> {
    let mine = seen.request.by.uuid == me.uuid || owner.is(me);
    if !mine || !seen.request.state.open() {
        return Err(Error::Refused);
    }
    let mut request = seen.request.clone();
    request.state = State::Withdrawn;
    request.decided_by = Some(me.name.clone());
    write(remote, &request, &Condition::Unchanged(seen.etag.clone())).map(|_| ())
}

/// What became of an approved request, written by the game that carried it out (or couldn't).
pub(crate) fn finish(remote: Remote<'_>, seen: &Seen, state: State, note: Option<String>) {
    let mut request = seen.request.clone();
    request.state = state;
    request.note = note;
    if let Err(e) = write(remote, &request, &Condition::Unchanged(seen.etag.clone())) {
        log::info!("Shared worlds: the load request isn't marked {state:?}: {e}");
    }
}

/// Makes `backup` the world's newest copy, as the new copy `id` of the game holding the lease
/// (`held`), for the request `request`; the lease says what was loaded and what it replaced.
/// Nothing is uploaded: the new copy points into the packs the backup does, each of which must
/// still be there.
pub fn load(
    remote: Remote<'_>,
    held: &mut Held,
    backup: GenId,
    id: GenId,
    by: &Member,
    request: &str,
) -> Result<Manifest> {
    let from = remote.manifest(backup)?;
    from.check()?;
    let prefix = remote.key("");
    let packs: BTreeSet<String> = remote
        .store
        .list(&remote.key("pack/"))?
        .into_iter()
        .filter_map(|o| o.key.strip_prefix(&prefix).map(str::to_owned))
        .collect();
    if let Some(missing) = from.packs.iter().find(|p| !packs.contains(*p)) {
        return Err(Error::Store(format!(
            "this backup is no longer complete in the storage ({missing} is gone)"
        )));
    }
    let copy = Manifest {
        version: VERSION,
        id,
        parent: Some(backup),
        by: by.name.clone(),
        packs: from.packs,
        files: from.files,
    };
    copy.check()?;
    remote.store.put(
        &remote.key(&id.manifest_key()),
        &copy.to_bytes()?,
        &Condition::Absent,
    )?;
    let at = now();
    if let Err(e) = stamp(remote, id, Kind::Loaded(backup), &by.name, at) {
        log::info!("Shared worlds: the loaded copy isn't stamped: {e}");
    }
    let replaced = held.lease().generation;
    held.commit_loaded(
        id,
        Loaded {
            backup,
            replaced,
            by: by.name.clone(),
            request: request.to_owned(),
            at,
        },
    )?;
    Ok(copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MemoryStore, Store};

    const PREFIX: &str = "w/0123456789abcdef/";

    fn member(n: u32) -> Member {
        Member {
            uuid: format!("{n:032x}"),
            name: format!("Player{n}"),
            reachable: false,
        }
    }

    fn remote(store: &MemoryStore) -> Remote<'_> {
        Remote {
            store,
            prefix: PREFIX,
        }
    }

    /// The owner alone chooses how often a backup is kept; the choice survives the record being
    /// read by a game that doesn't know it, and a world without one keeps the default.
    #[test]
    fn the_owner_chooses_how_often_backups_are_kept() {
        let store = MemoryStore::new();
        let remote = remote(&store);
        // No record yet: the default, and nobody may choose.
        assert_eq!(every(remote).unwrap(), EVERY_DEFAULT);
        assert!(matches!(set_every(remote, &member(1), 10), Err(Error::Refused)));
        assert!(record_owner(remote, &member(1)).unwrap());
        assert_eq!(every(remote).unwrap(), EVERY_DEFAULT);
        // Someone else: refused, nothing changes.
        assert!(matches!(set_every(remote, &member(2), 30), Err(Error::Refused)));
        assert_eq!(every(remote).unwrap(), EVERY_DEFAULT);
        // Not a choice the game offers: refused.
        assert!(matches!(set_every(remote, &member(1), 7), Err(Error::Format(_))));
        for minutes in EVERY {
            set_every(remote, &member(1), minutes).unwrap();
            assert_eq!(every(remote).unwrap(), minutes);
        }
        // The owner is kept, and recording it again changes nothing.
        assert_eq!(owner(remote).unwrap(), Owner::Known(member(1)));
        assert!(!record_owner(remote, &member(2)).unwrap());
        assert_eq!(every(remote).unwrap(), 60);
        // A record from before the choice existed reads, as the default.
        let old = br#"{"version":1,"owner":{"uuid":"00000000000000000000000000000001","name":"Player1"},"created":5}"#;
        let (_, etag) = meta(remote).unwrap().unwrap();
        store
            .put(&remote.key(META), old, &Condition::Unchanged(etag))
            .unwrap();
        assert_eq!(every(remote).unwrap(), EVERY_DEFAULT);
        set_every(remote, &member(1), 15).unwrap();
        assert_eq!(every(remote).unwrap(), 15);
        // A value no game offers is read as the nearest one that is.
        let odd = |every| {
            every_of(Some(&WorldMeta {
                version: 1,
                owner: member(1),
                created: 0,
                every: Some(every),
            }))
        };
        assert_eq!((odd(0), odd(3), odd(11), odd(1000)), (2, 2, 10, 60));
    }

    #[test]
    fn stamps_read_back_whatever_the_name() {
        let id = GenId {
            epoch: 7,
            seq: 12,
            day: 20261114,
        };
        let from = GenId {
            epoch: 3,
            seq: 4,
            day: 20261101,
        };
        for kind in [
            Kind::First,
            Kind::Checkpoint,
            Kind::Left,
            Kind::Loaded(from),
        ] {
            for name in ["Alex", "Steve_2", "Ünïcode-name and spaces", ""] {
                let key = stamp_key(id, kind, name, 1_763_100_000);
                let (of, at, back, by) = parse_stamp(&key).unwrap();
                assert_eq!((of, at, back), ((7, 12), 1_763_100_000, kind), "{key}");
                assert!(
                    !by.is_empty() && by.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                );
            }
        }
        assert_eq!(parse_stamp("stamp/x-1-2-c-A"), None);
        assert_eq!(parse_stamp("stamp/0000000001-000001-5-z-A"), None);
    }

    #[test]
    fn the_owner_is_recorded_once_and_a_world_from_before_is_named_by_its_first_copy() {
        let store = MemoryStore::new();
        // Before owners were kept: the first copy's maker.
        let first = Manifest {
            version: VERSION,
            id: GenId {
                epoch: 1,
                seq: 1,
                day: 20261101,
            },
            parent: None,
            by: "Player1".into(),
            packs: Vec::new(),
            files: Vec::new(),
        };
        assert_eq!(owner(remote(&store)).unwrap(), Owner::Unknown);
        store
            .put(
                &format!("{PREFIX}{}", first.id.manifest_key()),
                &first.to_bytes().unwrap(),
                &Condition::Always,
            )
            .unwrap();
        let named = owner(remote(&store)).unwrap();
        assert_eq!(named, Owner::Named("Player1".into()));
        assert!(named.is(&member(1)) && !named.is(&member(2)));
        assert!(record_owner(remote(&store), &member(1)).unwrap());
        assert!(!record_owner(remote(&store), &member(2)).unwrap());
        let known = owner(remote(&store)).unwrap();
        assert_eq!(known, Owner::Known(member(1)));
        // The same name on another account isn't the owner.
        let mut other = member(9);
        other.name = "Player1".into();
        assert!(!known.is(&other));
    }

    #[test]
    fn one_request_at_a_time_the_owner_decides_and_its_asker_may_withdraw_it() {
        let store = MemoryStore::new();
        let owner = Owner::Known(member(1));
        let backup = GenId {
            epoch: 2,
            seq: 3,
            day: 20261110,
        };
        let asked = ask(remote(&store), backup, &member(2), &owner, false).unwrap();
        assert_eq!(asked.state, State::Asked);
        assert!(matches!(
            ask(remote(&store), backup, &member(3), &owner, false),
            Err(Error::Busy)
        ));
        let seen = request(remote(&store)).unwrap().unwrap();
        // Only the owner decides.
        assert!(decide(remote(&store), &seen, &member(2), &owner, true).is_err());
        let declined = decide(remote(&store), &seen, &member(1), &owner, false).unwrap();
        assert_eq!(declined.state, State::Declined);
        // A decided request is stale for anyone holding the old tag.
        assert!(decide(remote(&store), &seen, &member(1), &owner, true).is_err());
        // Once it is decided, another may be made; the owner's own is approved at once.
        let mine = ask(remote(&store), backup, &member(1), &owner, true).unwrap();
        assert_eq!(mine.state, State::Approved);
        assert!(mine.undo);
        let seen = request(remote(&store)).unwrap().unwrap();
        assert!(withdraw(remote(&store), &seen, &member(3), &owner).is_err());
        withdraw(remote(&store), &seen, &member(1), &owner).unwrap();
        assert_eq!(
            request(remote(&store)).unwrap().unwrap().request.state,
            State::Withdrawn
        );
        // The asker may take back their own.
        ask(remote(&store), backup, &member(2), &owner, false).unwrap();
        let seen = request(remote(&store)).unwrap().unwrap();
        withdraw(remote(&store), &seen, &member(2), &owner).unwrap();
    }

    #[test]
    fn a_request_written_from_an_older_game_reads() {
        let text = r#"{"version":1,"id":"ab","backup":{"epoch":1,"seq":2,"day":20261101},"by":{"uuid":"00","name":"A"},"at":5,"state":"asked"}"#;
        let r: Request = serde_json::from_str(text).unwrap();
        assert!(!r.undo && r.decided_by.is_none() && r.state.open());
    }
}
