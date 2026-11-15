//! A shared world as the game drives it: creating one, taking it to host, keeping its copy in
//! the storage up to date while it runs, handing it on, and watching it while someone else hosts.
//!
//! The game owns the world itself (opening it, saving it, the invite code that joins it). This
//! module owns everything about the copy and the lease, on its own threads, and the game polls
//! it once a tick. The order of things that keeps a save whole:
//!
//! - **Taking the world**: the lease first, then the newest copy is brought onto this device,
//!   then the world opens. A copy this device made that never reached the storage (a host that
//!   crashed, or lost the world to a takeover) is kept as its own world first, never overwritten.
//! - **While it runs**: [`Hosting::capture`] is called with the world held still and fully on
//!   disk; it notes what changed and copies those files aside in a moment, and the upload runs
//!   behind the game from the copies. One upload at a time; a copy counts once the lease names it.
//! - **Leaving**: the world closes first, then its last copy goes up, then the lease is handed on.
//!   Until that has worked the lease stays this game's, so nothing is lost if the network is down:
//!   the next start on this device carries on from its own files.
use crate::{
    Error, Result,
    apply::{Fetched, Layout, fetch, recover},
    backups,
    invite::WorldInvite,
    lease::{Claim, Held, Lease, Leases, Member, Seen, State, Timing, Watch, turn},
    manifest::{GenId, Manifest, WORK},
    retention,
    s3::self_test,
    scan::{LocalIndex, absorb, changes, rescan, stage, under, walk, write_atomic},
    store::Store,
    sync::{Progress, Remote, publish},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Today as `yyyymmdd`, for a generation's day.
fn today() -> u32 {
    use chrono::Datelike;
    let date = chrono::Utc::now().date_naive();
    u32::try_from(date.year()).unwrap_or(0) * 10_000 + date.month() * 100 + date.day()
}

/// What a world folder says about the shared world it is a copy of. Holds no key: the invite
/// is kept in the game's secret store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub world: String,
    pub name: String,
}

impl Marker {
    pub fn read(world: &Path) -> Option<Self> {
        serde_json::from_slice(&fs::read(Layout::new(world).marker()).ok()?).ok()
    }

    pub fn write(&self, world: &Path) -> Result<()> {
        Ok(write_atomic(
            &Layout::new(world).marker(),
            &serde_json::to_vec(self)?,
        )?)
    }
}

/// Keeps other games on this device (and this game's other threads) off a world's files while
/// they are being changed: brought to a copy, or played.
pub struct FolderLock {
    _file: File,
}

impl FolderLock {
    pub fn take(world: &Path) -> Result<Self> {
        let work = world.join(WORK);
        fs::create_dir_all(&work)?;
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(work.join("lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(Error::Busy),
            // A file system without locks (Android's shared storage): as the game opens its
            // worlds there, without one.
            Err(TryLockError::Error(e)) if e.kind() == io::ErrorKind::Unsupported => {
                log::warn!("Shared worlds: {} can't be locked: {e}", world.display());
                Ok(Self { _file: file })
            }
            Err(TryLockError::Error(e)) => Err(e.into()),
        }
    }
}

/// Copies a world folder's own files (not the lock a running game keeps, nor the shared world's
/// working folder) into `to`, which must not exist yet.
pub fn copy_world(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        )));
    }
    let part = to.with_file_name(format!(
        "{}.part",
        to.file_name().and_then(|n| n.to_str()).unwrap_or("world")
    ));
    let _ = fs::remove_dir_all(&part);
    for file in walk(from)? {
        let target = under(&part, &file.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(under(from, &file.path), target)?;
    }
    fs::create_dir_all(&part)?;
    fs::rename(&part, to)?;
    Ok(())
}

/// Whether the folder holds anything its last copy from the storage doesn't: progress made here
/// that never reached the storage. `index` must be up to date with the folder.
pub fn unpublished(layout: &Layout, index: &LocalIndex) -> bool {
    match layout.load_base() {
        None => !index.files.is_empty(),
        Some(base) => {
            base.files.len() != index.files.len()
                || base.files.iter().any(|file| {
                    index
                        .files
                        .get(&file.path)
                        .is_none_or(|local| local.size != file.size || local.sha256 != file.sha256)
                })
        }
    }
}

/// The player who takes the world when its host leaves: the first one in the list (the order
/// they joined) that friends can reach, else the first one.
pub fn successor(players: &[Member], host: &str) -> Option<String> {
    let others = || players.iter().filter(|p| p.uuid != host);
    others()
        .find(|p| p.reachable)
        .or_else(|| others().next())
        .map(|p| p.uuid.clone())
}

/// What a player who isn't hosting does next, from the lease as last read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// Nothing yet: the world is getting ready, or its host may still come back.
    Wait,
    /// The world is open: connect with this code.
    Join {
        code: String,
        /// Where the host's public tunnel is reached, when it has one.
        tunnel: Option<String>,
    },
    /// Take the world and host it.
    Host(Claim),
    /// The world is no longer in its storage.
    Gone,
}

/// `quiet` is how long this exact lease has stood still on this device's timer ([`Watch`]);
/// `device` is this one's (a game that isn't hosting the world asks).
pub fn next_step(
    lease: Option<&Lease>,
    quiet: Duration,
    me: &str,
    device: &str,
    timing: &Timing,
) -> Next {
    let Some(lease) = lease else {
        return Next::Gone;
    };
    // This device's own game held it and stopped: nobody else can be hosting it, so there is
    // nothing to wait for. The same player picks it up again as it was (`Leases::resume`);
    // another account on this device takes it over, the old progress kept apart.
    if lease.left_here(device) {
        return Next::Host(if lease.left_by(me, device) {
            Claim::Free
        } else {
            Claim::Takeover
        });
    }
    let turn = turn(lease, me);
    let host_is_me = lease.host.as_ref().is_some_and(|h| h.uuid == me);
    // As long as the host's own game needs (an older game's lease says nothing: its 90 s).
    let dead = timing.after(timing.dead_for(lease), quiet, turn);
    match lease.state {
        State::Hosting | State::Starting if dead => Next::Host(Claim::Takeover),
        State::Hosting => match &lease.code {
            Some(code) if !host_is_me => Next::Join {
                code: code.clone(),
                tunnel: lease.tunnel_at.clone().filter(|_| lease.tunnel),
            },
            _ => Next::Wait,
        },
        State::Starting => Next::Wait,
        State::Handoff => match lease.successor.as_deref() {
            Some(successor) if successor == me => Next::Host(Claim::Free),
            None => Next::Host(Claim::Free),
            Some(_) if timing.after(timing.handoff_wait, quiet, turn) => {
                Next::Host(Claim::Takeover)
            }
            Some(_) => Next::Wait,
        },
        State::Closed => Next::Host(Claim::Free),
    }
}

/// One shared world on this device: where its copy is, and who this player is.
pub struct Context {
    store: Arc<dyn Store>,
    prefix: String,
    lease_key: String,
    folder: PathBuf,
    pub me: Member,
    pub device: String,
    pub timing: Timing,
}

impl Context {
    pub fn new(
        store: Arc<dyn Store>,
        invite: &WorldInvite,
        folder: PathBuf,
        me: Member,
        device: String,
    ) -> Self {
        let prefix = invite.prefix();
        Self {
            store,
            lease_key: format!("{prefix}lease"),
            prefix,
            folder,
            me,
            device,
            timing: Timing::REAL,
        }
    }

    pub fn remote(&self) -> Remote<'_> {
        Remote {
            store: &*self.store,
            prefix: &self.prefix,
        }
    }

    pub fn leases(&self) -> Leases {
        Leases::new(self.store.clone(), &self.lease_key)
    }

    pub fn layout(&self) -> Layout {
        Layout::new(&self.folder)
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The lease as the storage has it now: who hosts, how many play.
    pub fn status(&self) -> Result<Option<Seen>> {
        self.leases().read()
    }

    /// The next free `seq` of `epoch`: past every copy and every pack ever begun under it, so a
    /// copy that failed half-way is never written over.
    fn next_seq(&self, epoch: u64) -> Result<u32> {
        let remote = self.remote();
        let mut last = retention::generations(remote)?
            .iter()
            .filter(|g| g.epoch == epoch)
            .map(|g| g.seq)
            .max()
            .unwrap_or(0);
        let prefix = remote.key("");
        for object in self.store.list(&remote.key("pack/"))? {
            if let Some((e, seq)) = object.key.strip_prefix(&prefix).and_then(GenId::of_pack)
                && e == epoch
            {
                last = last.max(seq);
            }
        }
        Ok(last + 1)
    }
}

/// Makes the world in `ctx`'s folder a shared world: proves the bucket can hold one, uploads the
/// folder (the world must be closed) as the first copy, and leaves the lease closed so whoever
/// opens it first hosts.
pub fn create(ctx: &Context, name: &str, progress: Progress<'_>) -> Result<GenId> {
    self_test(&*ctx.store, &ctx.prefix)?;
    let _lock = FolderLock::take(&ctx.folder)?;
    let leases = ctx.leases();
    let mut held = leases.claim(None, &ctx.me, &ctx.device, Claim::Free)?;
    let made = (|| {
        // Who shared it: the one who says yes to loading a backup.
        backups::record_owner(ctx.remote(), &ctx.me)?;
        let layout = ctx.layout();
        recover(&layout)?;
        let mut index = LocalIndex::default();
        rescan(&ctx.folder, &mut index)?;
        if !index.files.contains_key("level.dat") {
            return Err(Error::Format(
                "this folder is not a world: it has no level.dat".into(),
            ));
        }
        let id = GenId {
            epoch: held.lease().epoch,
            seq: 1,
            day: today(),
        };
        let folder = ctx.folder.clone();
        let manifest = publish(
            ctx.remote(),
            None,
            &index,
            &|p| under(&folder, p),
            id,
            &ctx.me.name,
            progress,
        )?;
        held.commit(id)?;
        note(ctx, id, backups::Kind::First);
        layout.save_base(&manifest)?;
        index.save(&layout.index())?;
        Marker {
            world: ctx
                .prefix
                .trim_start_matches("w/")
                .trim_end_matches('/')
                .to_owned(),
            name: name.to_owned(),
        }
        .write(&ctx.folder)?;
        held.release()?;
        Ok(id)
    })();
    if made.is_err() {
        // Nobody knows this world yet (its lease was new, and the invite isn't out): take all of
        // it out of the storage again, so making it can be tried again from the start.
        if let Ok(objects) = ctx.store.list(&ctx.prefix) {
            for object in objects {
                let _ = ctx.store.delete(&object.key);
            }
        }
        let _ = ctx.store.delete(&ctx.lease_key);
    }
    made
}

/// What a guest's device does in the background: brings its copy to the newest one, so that
/// hosting later starts with almost nothing to download. Leaves alone a folder that holds
/// progress of its own, or one another thread is using. Returns what it fetched, if anything.
pub fn prefetch(ctx: &Context, progress: Progress<'_>) -> Result<Option<Fetched>> {
    let _lock = match FolderLock::take(&ctx.folder) {
        Ok(lock) => lock,
        Err(Error::Busy) => return Ok(None),
        Err(e) => return Err(e),
    };
    let layout = ctx.layout();
    recover(&layout)?;
    let Some(seen) = ctx.status()? else {
        return Ok(None);
    };
    let Some(newest) = seen.lease.generation else {
        return Ok(None);
    };
    if layout.load_base().is_some_and(|base| base.id == newest) {
        return Ok(None);
    }
    let mut index = LocalIndex::load(&layout.index());
    rescan(&ctx.folder, &mut index)?;
    if unpublished(&layout, &index) && layout.load_base().is_some() {
        return Ok(None);
    }
    let target = ctx.remote().manifest(newest)?;
    fetch(ctx.remote(), &target, &layout, &mut index, progress).map(Some)
}

/// How a game asks for the world.
pub enum Start {
    /// It is closed, never opened, or handed to this player. A world this player was hosting on
    /// this device when the game stopped is picked up where it was.
    Free,
    /// Its host stopped: this is the lease that was watched standing still. If it changed since,
    /// the host came back and the world isn't taken.
    Takeover(Seen),
}

/// The lease while this game hosts: a thread keeps its heartbeat, and every write goes through
/// here one at a time.
struct HostLease {
    held: Mutex<Held>,
    /// The lease as last written, for readers that mustn't wait on a write under way.
    snapshot: Mutex<Lease>,
    players: Mutex<(Vec<Member>, bool)>,
    open: Mutex<Option<(String, Option<String>)>>,
    /// The app is away from the screen, and whether that is yet to be written.
    away: Mutex<(bool, bool)>,
    last_ok: Mutex<Instant>,
    fenced: AtomicBool,
    stop: AtomicBool,
    timing: Timing,
    /// When the game last said it still runs the world ([`Hosting::alive`]); None until it
    /// first does. A game that stops saying so (it hangs, or lost the world without letting
    /// it go) stops the heartbeat with it, so the others take the world rather than see it
    /// played by nobody.
    pulse: Mutex<Option<Instant>>,
    /// The world is closed and its last copy goes up ([`Hosting::leave`]): the heartbeat
    /// keeps the world until then without the game's word.
    leaving: AtomicBool,
    /// The heartbeat stands still for the want of the game's word (logged once each time).
    stalled: AtomicBool,
    /// Minutes between backups, as the world's owner chose them ([`backups::every`]), read
    /// again every [`EVERY_READ_BEATS`] heartbeats; and whether that changed since the game
    /// last asked.
    every: AtomicU32,
    every_changed: AtomicBool,
}

/// How often a host reads the owner's choice of minutes between backups again, in heartbeats
/// (a minute).
const EVERY_READ_BEATS: u32 = 10;

impl HostLease {
    fn write(&self, change: impl FnOnce(&mut Held) -> Result<()>) -> Result<()> {
        if self.fenced.load(Ordering::SeqCst) {
            return Err(Error::Fenced);
        }
        let sent = Instant::now();
        let mut held = locked(&self.held);
        match change(&mut held) {
            Ok(()) => {
                *locked(&self.snapshot) = held.lease().clone();
                let mut last = locked(&self.last_ok);
                *last = (*last).max(sent);
                Ok(())
            }
            Err(Error::Fenced) => {
                self.fenced.store(true, Ordering::SeqCst);
                Err(Error::Fenced)
            }
            Err(e) => Err(e),
        }
    }

    fn quiet(&self) -> Duration {
        locked(&self.last_ok).elapsed()
    }

    fn set_every(&self, minutes: u32) {
        if self.every.swap(minutes, Ordering::SeqCst) != minutes {
            self.every_changed.store(true, Ordering::SeqCst);
        }
    }

    /// The heartbeat thread's work: the code once it is known, the players when they change,
    /// and a heartbeat when one is due. A failed write is tried again shortly.
    fn beat(&self, last_try: &mut Option<Instant>) {
        let retry = self.timing.heartbeat / 5;
        if last_try.is_some_and(|t| t.elapsed() < retry) {
            return;
        }
        let open = locked(&self.open).clone();
        let (players, changed) = locked(&self.players).clone();
        let (away, away_changed) = *locked(&self.away);
        // Out of sight, the system may hold the game's frames back: the away flag covers that.
        let stalled = !away
            && !self.leaving.load(Ordering::SeqCst)
            && locked(&self.pulse).is_some_and(|p| p.elapsed() > self.timing.heartbeat * 2);
        if stalled != self.stalled.swap(stalled, Ordering::SeqCst) {
            if stalled {
                log::warn!(
                    "Shared worlds: the game stopped running the world: the lease's heartbeat waits for it"
                );
            } else {
                log::info!("Shared worlds: the game runs the world again: the heartbeat goes on");
            }
        }
        if stalled && !away_changed {
            return;
        }
        let due = self.quiet() >= self.timing.heartbeat;
        // The app leaving the screen first: the system may suspend it any moment after.
        let result = if away_changed {
            self.write(|held| held.away(away)).map(|()| {
                let mut now = locked(&self.away);
                if now.0 == away {
                    now.1 = false;
                }
            })
        } else if let Some(asked) = open {
            self.write(|held| held.open(&asked.0, asked.1.as_deref()))
                .map(|()| {
                    let mut open = locked(&self.open);
                    // A newer code that came meanwhile is written next.
                    if open.as_ref() == Some(&asked) {
                        *open = None;
                    }
                })
        } else if changed || due {
            self.write(|held| held.heartbeat(&players)).map(|()| {
                let mut listed = locked(&self.players);
                if listed.0 == players {
                    listed.1 = false;
                }
            })
        } else {
            return;
        };
        *last_try = Some(Instant::now());
        if let Err(e) = result
            && !matches!(e, Error::Fenced)
        {
            log::warn!("Shared worlds: the lease wasn't written: {e}");
        }
    }
}

/// An upload's place: the index and the last committed copy move into the uploading thread and
/// back.
struct Uploads {
    index: LocalIndex,
    previous: Option<Manifest>,
    next_seq: u32,
    busy: bool,
    done: Option<Result<GenId>>,
    commits: u32,
}

/// This game hosts the world.
pub struct Hosting {
    ctx: Arc<Context>,
    lease: Arc<HostLease>,
    uploads: Arc<(Mutex<Uploads>, Condvar)>,
    heart: Mutex<Option<JoinHandle<()>>>,
    epoch: u64,
    /// Where progress made here that never reached the storage was kept, when hosting began on
    /// top of someone else's newer copy.
    pub recovered: Option<PathBuf>,
    /// The copy this device was brought to before the world opens.
    pub fetched: Option<Fetched>,
    /// Held while this game hosts; let go as it stops.
    lock: Mutex<Option<FolderLock>>,
}

/// Takes the world to host: the lease, then the newest copy onto this device. When this returns
/// the folder is ready to open. `recover_into` names a new folder for progress of this device's
/// that is about to be replaced by a newer copy.
pub fn host(
    ctx: Arc<Context>,
    start: Start,
    recover_into: &dyn Fn() -> PathBuf,
    progress: Progress<'_>,
) -> Result<Hosting> {
    let lock = FolderLock::take(&ctx.folder)?;
    let leases = ctx.leases();
    let (mut held, mut resumed) = match start {
        Start::Takeover(seen) => (
            leases.claim(Some(&seen), &ctx.me, &ctx.device, Claim::Takeover)?,
            false,
        ),
        Start::Free => {
            let Some(seen) = leases.read()? else {
                return Err(Error::Store(
                    "this world is no longer in its storage".into(),
                ));
            };
            match leases.resume(&seen, &ctx.me, &ctx.device) {
                Ok(held) => (held, true),
                Err(Error::Refused) => (
                    leases.claim(Some(&seen), &ctx.me, &ctx.device, Claim::Free)?,
                    false,
                ),
                // The lease changed while it was being picked up: read it again.
                Err(Error::Fenced) => return Err(Error::Refused),
                Err(e) => return Err(e),
            }
        }
    };
    // A backup the owner approved loading: this game holds the world now, so it loads it,
    // before anything is brought onto this device (which then brings the backup).
    held.set_pace(&ctx.timing);
    if carry_out(&ctx, &mut held).is_some() {
        // The backup replaces whatever this device had, its own unsent progress included,
        // which is kept apart as for any newer copy.
        resumed = false;
    }
    let epoch = held.lease().epoch;
    let newest = held.lease().generation;
    let lease = Arc::new(HostLease {
        snapshot: Mutex::new(held.lease().clone()),
        held: Mutex::new(held),
        players: Mutex::new((vec![ctx.me.clone()], false)),
        open: Mutex::new(None),
        away: Mutex::new((false, false)),
        last_ok: Mutex::new(Instant::now()),
        fenced: AtomicBool::new(false),
        stop: AtomicBool::new(false),
        timing: ctx.timing,
        pulse: Mutex::new(None),
        leaving: AtomicBool::new(false),
        stalled: AtomicBool::new(false),
        // Unreadable now: the default, until the next read says otherwise.
        every: AtomicU32::new(backups::every(ctx.remote()).unwrap_or(backups::EVERY_DEFAULT)),
        every_changed: AtomicBool::new(false),
    });
    // The heartbeat runs from now: bringing the copy over may take a while, and the world is
    // this game's meanwhile.
    let heart = {
        let lease = lease.clone();
        let ctx = ctx.clone();
        let step = (ctx.timing.heartbeat / 15).max(Duration::from_millis(10));
        let read_every = ctx.timing.heartbeat * EVERY_READ_BEATS;
        std::thread::Builder::new()
            .name("shared-worlds-lease".into())
            .spawn(move || {
                let mut last_try = None;
                let mut every_read = Instant::now();
                while !lease.stop.load(Ordering::SeqCst) && !lease.fenced.load(Ordering::SeqCst) {
                    lease.beat(&mut last_try);
                    // The owner may choose another pace while someone else hosts.
                    if every_read.elapsed() >= read_every {
                        every_read = Instant::now();
                        if let Ok(minutes) = backups::every(ctx.remote()) {
                            lease.set_every(minutes);
                        }
                    }
                    std::thread::sleep(step);
                }
            })?
    };
    let mut hosting = Hosting {
        ctx: ctx.clone(),
        lease,
        uploads: Arc::new((
            Mutex::new(Uploads {
                index: LocalIndex::default(),
                previous: None,
                next_seq: 1,
                busy: false,
                done: None,
                commits: 0,
            }),
            Condvar::new(),
        )),
        heart: Mutex::new(Some(heart)),
        epoch,
        recovered: None,
        fetched: None,
        lock: Mutex::new(Some(lock)),
    };
    match hosting.prepare(resumed, newest, recover_into, progress) {
        Ok(()) => Ok(hosting),
        Err(e) => {
            // The world couldn't be made ready: give it back as it was. Not a world this device
            // was hosting with progress of its own, which stays this device's to carry on.
            if !resumed {
                let _ = hosting.lease.write(|held| held.release());
            }
            Err(e)
        }
    }
}

impl Hosting {
    fn prepare(
        &mut self,
        resumed: bool,
        newest: Option<GenId>,
        recover_into: &dyn Fn() -> PathBuf,
        progress: Progress<'_>,
    ) -> Result<()> {
        let ctx = &self.ctx;
        let layout = ctx.layout();
        recover(&layout)?;
        fs::create_dir_all(&ctx.folder)?;
        let mut index = LocalIndex::load(&layout.index());
        rescan(&ctx.folder, &mut index)?;
        let previous = match newest {
            Some(id) => Some(ctx.remote().manifest(id)?),
            None => None,
        };
        if !resumed && let Some(target) = &previous {
            if unpublished(&layout, &index) && !index.files.is_empty() {
                let to = recover_into();
                copy_world(&ctx.folder, &to)?;
                log::info!(
                    "Shared worlds: this device's own progress is kept in {}",
                    to.display()
                );
                self.recovered = Some(to);
            }
            self.fetched = Some(fetch(ctx.remote(), target, &layout, &mut index, progress)?);
        }
        if previous.is_none() && index.files.is_empty() {
            return Err(Error::Store(
                "this world has no saved copy to start from".into(),
            ));
        }
        // Whatever differs from the newest copy (a resumed host's own progress) waits in the
        // staging folder for the first upload, as a checkpoint's files do.
        let staging = layout.staging();
        let _ = fs::remove_dir_all(&staging);
        for (path, file) in &index.files {
            let same = previous
                .as_ref()
                .and_then(|m| m.file(path))
                .is_some_and(|m| m.size == file.size && m.sha256 == file.sha256);
            if !same {
                let to = under(&staging, path);
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(under(&ctx.folder, path), to)?;
            }
        }
        index.save(&layout.index())?;
        let next_seq = ctx.next_seq(self.epoch)?;
        let mut uploads = locked(&self.uploads.0);
        uploads.index = index;
        uploads.previous = previous;
        uploads.next_seq = next_seq;
        Ok(())
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Another game took the world: stop the world at once, and leave without publishing.
    pub fn fenced(&self) -> bool {
        self.lease.fenced.load(Ordering::SeqCst)
    }

    /// The lease hasn't been written for long enough that another game may soon take the world:
    /// hold the world still until this turns false again.
    pub fn must_freeze(&self) -> bool {
        self.fenced() || self.lease.quiet() >= self.ctx.timing.self_fence
    }

    /// Minutes between the backups this host keeps (its checkpoints), as the owner chose.
    pub fn backup_every(&self) -> u32 {
        self.lease.every.load(Ordering::SeqCst)
    }

    /// The owner chose another pace on this device: it counts at once.
    pub fn set_backup_every(&self, minutes: u32) {
        self.lease.set_every(minutes);
    }

    /// Whether the pace changed since this was last asked (the next checkpoint's wait is
    /// worked out again).
    pub fn backup_every_changed(&self) -> bool {
        self.lease.every_changed.swap(false, Ordering::SeqCst)
    }

    /// The game still runs the world: call it every frame while hosting (and closing it). Once
    /// called, the heartbeat goes on only while it keeps being called (or the app is away, or
    /// leaving): a game that hangs or loses the world stops it, and the others take over.
    pub fn alive(&self) {
        *locked(&self.lease.pulse) = Some(Instant::now());
    }

    /// The app left the screen (`true`), or came back: the lease says so at once, so nobody is
    /// shown playing a world its host's system may have suspended.
    pub fn set_away(&self, away: bool) {
        let mut now = locked(&self.lease.away);
        if now.0 != away {
            *now = (away, true);
        }
    }

    /// The world is open and `code` joins it; `tunnel` is where its public tunnel is reached.
    pub fn open(&self, code: &str, tunnel: Option<&str>) {
        *locked(&self.lease.open) = Some((code.to_owned(), tunnel.map(str::to_owned)));
    }

    /// Who is in the world now, the host first.
    pub fn set_players(&self, players: Vec<Member>) {
        let mut held = locked(&self.lease.players);
        if held.0 != players {
            *held = (players, true);
        }
    }

    /// The lease as this game last wrote it.
    pub fn lease(&self) -> Lease {
        locked(&self.lease.snapshot).clone()
    }

    /// Whether an upload is running: a checkpoint now would be skipped.
    pub fn uploading(&self) -> bool {
        locked(&self.uploads.0).busy
    }

    /// The outcome of the upload that finished since the last call. [`Error::Fenced`] ends the
    /// hosting; any other error is tried again by the next checkpoint.
    pub fn poll(&self) -> Option<Result<GenId>> {
        locked(&self.uploads.0).done.take()
    }

    /// The newest copy the storage holds of this world.
    pub fn committed(&self) -> Option<GenId> {
        locked(&self.uploads.0).previous.as_ref().map(|m| m.id)
    }

    /// A checkpoint. Call it while the world is held still with everything on disk (saved, its
    /// writers idle): it finds the files that changed and copies them aside, which takes a moment,
    /// and then the world may run on while they upload. `unchanged` says whether nothing wrote
    /// to the world's files while they were copied (the game's own count of its writes): when
    /// something did, the copies are given up. False when nothing was started (the last upload
    /// still runs, or a write came in between): try at the next checkpoint.
    pub fn capture(&self, unchanged: &dyn Fn() -> bool) -> Result<bool> {
        if self.fenced() {
            return Err(Error::Fenced);
        }
        let (mutex, _) = &*self.uploads;
        let mut uploads = locked(mutex);
        if uploads.busy {
            return Ok(false);
        }
        let layout = self.ctx.layout();
        // Every file's size and time of change, before and after the copies: whatever the game
        // says, a file that moved meanwhile gives the copies up.
        let before = walk(&self.ctx.folder)?;
        let found = changes(&self.ctx.folder, &uploads.index)?;
        stage(&self.ctx.folder, &layout.staging(), &found)?;
        let still = unchanged();
        if !still || walk(&self.ctx.folder)? != before {
            // Staged copies that may be torn stay unused: the index still says these files
            // differ, so the next checkpoint copies them again.
            log::warn!(
                "Shared worlds: the world was written to during a checkpoint; it is taken again later"
            );
            return Ok(false);
        }
        let index = std::mem::take(&mut uploads.index);
        let previous = uploads.previous.clone();
        let seq = uploads.next_seq;
        uploads.next_seq += 1;
        uploads.busy = true;
        drop(uploads);
        let ctx = self.ctx.clone();
        let lease = self.lease.clone();
        let shared = self.uploads.clone();
        let epoch = self.epoch;
        let spawned = std::thread::Builder::new()
            .name("shared-worlds-upload".into())
            .spawn(move || {
                let mut index = index;
                let staging = ctx.layout().staging();
                let id = GenId {
                    epoch,
                    seq,
                    day: today(),
                };
                let outcome = upload(
                    &ctx,
                    &lease,
                    &mut index,
                    &found,
                    previous.as_ref(),
                    &staging,
                    id,
                );
                let (mutex, done) = &*shared;
                let mut uploads = locked(mutex);
                uploads.index = index;
                let mut tidy = false;
                uploads.done = Some(match outcome {
                    Ok(manifest) => {
                        uploads.previous = Some(manifest);
                        uploads.commits += 1;
                        // The first copy of a stay (the bucket may have filled meanwhile),
                        // then every 20.
                        tidy = uploads.commits == 1 || uploads.commits.is_multiple_of(20);
                        // Every staged file is in the copy just committed.
                        let _ = fs::remove_dir_all(&staging);
                        Ok(id)
                    }
                    Err(e) => Err(e),
                });
                uploads.busy = false;
                done.notify_all();
                drop(uploads);
                // Outside the lock: the game never waits on it. A newer upload may start
                // meanwhile; the sweep only removes what is older than `id` and unused by it.
                if tidy {
                    sweep(&ctx, &lease, id);
                }
            });
        if let Err(e) = spawned {
            // Nothing moved: the index is gone with the thread that never ran, so the next
            // checkpoint scans the whole folder again.
            let mut uploads = locked(mutex);
            uploads.busy = false;
            return Err(e.into());
        }
        Ok(true)
    }

    fn wait_idle(&self) {
        let (mutex, done) = &*self.uploads;
        let mut uploads = locked(mutex);
        while uploads.busy {
            uploads = done.wait(uploads).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// The host leaves. Call it once the world is closed and saved: its last copy goes up and
    /// the lease is handed to `successor` (a UUID), or closed. On an error the world is still
    /// this game's; call again, or [`Self::abandon`] to leave it for this device's next start.
    pub fn leave(&self, successor: Option<&str>, progress: Progress<'_>) -> Result<GenId> {
        // The upload may outlast the game's frames (in the background, say): the heartbeat
        // keeps the world till the last copy is in.
        self.lease.leaving.store(true, Ordering::SeqCst);
        self.wait_idle();
        if self.fenced() {
            return Err(Error::Fenced);
        }
        let ctx = self.ctx.clone();
        // Taken out, as an upload takes them, so nothing waits on this lock meanwhile.
        let (mut index, previous, seq) = {
            let mut uploads = locked(&self.uploads.0);
            uploads.busy = true;
            let seq = uploads.next_seq;
            uploads.next_seq += 1;
            (
                std::mem::take(&mut uploads.index),
                uploads.previous.clone(),
                seq,
            )
        };
        let id = GenId {
            epoch: self.epoch,
            seq,
            day: today(),
        };
        let lease = self.lease.clone();
        // Nobody is in the world from now on: the list says so and nobody dials it while the
        // last copy goes up. Not reaching the storage for this changes nothing else.
        let host = ctx.me.clone();
        // A code that came just before isn't written after it.
        *locked(&lease.open) = None;
        if let Err(e) = lease.write(|held| held.closing(&host)) {
            log::info!("Shared worlds: the lease doesn't say the world is closing yet: {e}");
        }
        let outcome: Result<Manifest> = (|| {
            let layout = ctx.layout();
            rescan(&ctx.folder, &mut index)?;
            let folder = ctx.folder.clone();
            let manifest = publish(
                ctx.remote(),
                previous.as_ref(),
                &index,
                &|p| under(&folder, p),
                id,
                &ctx.me.name,
                progress,
            )?;
            lease.write(|held| held.hand_off(id, successor))?;
            note(&ctx, id, backups::Kind::Left);
            layout.save_base(&manifest)?;
            index.save(&layout.index())?;
            let _ = fs::remove_dir_all(layout.staging());
            Ok(manifest)
        })();
        let (mutex, done) = &*self.uploads;
        let mut uploads = locked(mutex);
        uploads.index = index;
        uploads.busy = false;
        done.notify_all();
        let manifest = outcome?;
        uploads.previous = Some(manifest);
        drop(uploads);
        self.stop_heart();
        Ok(id)
    }

    /// The world couldn't be opened after all: give it back as it was.
    pub fn release(&self) -> Result<()> {
        self.wait_idle();
        let released = self.lease.write(|held| held.release());
        self.stop_heart();
        released
    }

    /// Stops hosting without handing the world on (the app is closing, or the world was taken).
    /// What this device has stays here; while nobody takes the world, the next start on this
    /// device carries on from it.
    pub fn abandon(&self) {
        self.stop_heart();
    }

    /// Stops the heartbeat and lets go of the world's files.
    fn stop_heart(&self) {
        self.lease.stop.store(true, Ordering::SeqCst);
        if let Some(heart) = locked(&self.heart).take() {
            let _ = heart.join();
        }
        locked(&self.lock).take();
    }

    /// Whether this game still keeps the lease's heartbeat.
    pub fn active(&self) -> bool {
        !self.lease.stop.load(Ordering::SeqCst) && !self.fenced()
    }
}

impl Drop for Hosting {
    fn drop(&mut self) {
        self.stop_heart();
    }
}

/// An upload from the staged copies: hashed, published, committed.
fn upload(
    ctx: &Context,
    lease: &HostLease,
    index: &mut LocalIndex,
    found: &crate::scan::Changes,
    previous: Option<&Manifest>,
    staging: &Path,
    id: GenId,
) -> Result<Manifest> {
    let layout = ctx.layout();
    absorb(index, found, &|p| under(staging, p))?;
    let stop = &lease.stop;
    let manifest = publish(
        ctx.remote(),
        previous,
        index,
        &|p| under(staging, p),
        id,
        &ctx.me.name,
        &mut |_, _| !stop.load(Ordering::SeqCst),
    )?;
    lease.write(|held| held.commit(id))?;
    note(ctx, id, backups::Kind::Checkpoint);
    layout.save_base(&manifest)?;
    index.save(&layout.index())?;
    Ok(manifest)
}

/// Stamps a copy just committed: when, by whom, why (only for showing backups).
fn note(ctx: &Context, id: GenId, kind: backups::Kind) {
    if let Err(e) = backups::stamp(ctx.remote(), id, kind, &ctx.me.name, backups::now()) {
        log::info!("Shared worlds: the copy {id:?} isn't stamped: {e}");
    }
}

/// Loads the backup an approved request names, `held` being this game's lease on the world:
/// the new copy's id when it did. A request already carried out (its finish not written) is
/// marked done; one that can't be is marked failed, and the world goes on from its newest copy.
fn carry_out(ctx: &Context, held: &mut Held) -> Option<GenId> {
    let seen = match backups::request(ctx.remote()) {
        Ok(Some(seen)) if seen.request.state == backups::State::Approved => seen,
        Ok(_) => return None,
        Err(e) => {
            log::warn!("Shared worlds: the load requests weren't read: {e}");
            return None;
        }
    };
    if held
        .lease()
        .loaded
        .as_ref()
        .is_some_and(|l| l.request == seen.request.id)
    {
        backups::finish(ctx.remote(), &seen, backups::State::Done, None);
        return None;
    }
    // Past every copy of this epoch (a host that picked its own lease up again has some).
    let epoch = held.lease().epoch;
    let seq = match ctx.next_seq(epoch) {
        Ok(seq) => seq,
        Err(e) => {
            log::warn!("Shared worlds: the backup wasn't loaded yet: {e}");
            return None;
        }
    };
    let id = GenId {
        epoch,
        seq,
        day: today(),
    };
    match backups::load(
        ctx.remote(),
        held,
        seen.request.backup,
        id,
        &seen.request.by,
        &seen.request.id,
    ) {
        Ok(_) => {
            log::info!(
                "Shared worlds: loaded the backup {:?} as {id:?}",
                seen.request.backup
            );
            backups::finish(ctx.remote(), &seen, backups::State::Done, None);
            Some(id)
        }
        Err(Error::Fenced) => None,
        Err(e) => {
            log::warn!("Shared worlds: the backup wasn't loaded: {e}");
            backups::finish(
                ctx.remote(),
                &seen,
                backups::State::Failed,
                Some(e.to_string()),
            );
            None
        }
    }
}

/// Carries out the approved request at once, when nobody hosts the world: a load needs no game
/// open (each player's copy follows the next time they play). [`Error::Refused`] while someone
/// hosts it or nothing is approved: then whoever takes the world next carries it out.
pub fn load_now(ctx: &Context) -> Result<GenId> {
    let leases = ctx.leases();
    let Some(seen) = leases.read()? else {
        return Err(Error::Store(
            "this world is no longer in its storage".into(),
        ));
    };
    let mut held = leases.claim(Some(&seen), &ctx.me, &ctx.device, Claim::Free)?;
    held.set_pace(&ctx.timing);
    let loaded = carry_out(ctx, &mut held);
    let released = held.release();
    match loaded {
        Some(id) => released.map(|()| id),
        None => {
            released?;
            Err(Error::Refused)
        }
    }
}

/// What a sweep must keep for loads: the copy the last load replaced (its undo), and the
/// backup an open request asks for.
fn needed_for_loads(ctx: &Context, lease: &HostLease) -> Result<Vec<GenId>> {
    let mut keep: Vec<GenId> = locked(&lease.snapshot)
        .loaded
        .iter()
        .filter_map(|l| l.replaced)
        .collect();
    if let Some(seen) = backups::request(ctx.remote())?
        && seen.request.state.open()
    {
        keep.push(seen.request.backup);
    }
    Ok(keep)
}

fn sweep(ctx: &Context, lease: &HostLease, current: GenId) {
    // Without knowing what a load needs, nothing is removed.
    let protected = match needed_for_loads(ctx, lease) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Shared worlds: old copies weren't removed: {e}");
            return;
        }
    };
    match retention::sweep(ctx.remote(), current, &protected, retention::BUDGET) {
        Ok(swept) => {
            if swept.packs > 0 || swept.manifests > 0 {
                log::info!(
                    "Shared worlds: removed {} old copies and {} packs ({} KiB); the bucket holds {} MB",
                    swept.manifests,
                    swept.packs,
                    swept.bytes / 1024,
                    swept.used / 1_000_000
                );
            }
            if swept.full {
                log::warn!(
                    "Shared worlds: the bucket holds {} MB, over the {} MB kept free, with only the backups that must stay",
                    swept.used / 1_000_000,
                    retention::BUDGET / 1_000_000
                );
            }
        }
        Err(e) => log::warn!("Shared worlds: old copies weren't removed: {e}"),
    }
}

/// Reads a world's request to load a backup, and its owner, every few seconds while it is
/// played: the host's game carries out an approved one, and every player's Friends screen shows
/// it (with the owner's answer buttons).
pub struct RequestWatch {
    latest: Arc<Mutex<Option<(Option<backups::Seen>, backups::Owner)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl RequestWatch {
    pub fn start(ctx: Arc<Context>) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (latest, stop) = (latest.clone(), stop.clone());
            std::thread::Builder::new()
                .name("shared-worlds-requests".into())
                .spawn(move || {
                    let every = ctx.timing.poll * 2;
                    let step = Duration::from_millis(50).min(every);
                    let mut next = Instant::now();
                    let mut owner: Option<backups::Owner> = None;
                    let mut reads = 0u32;
                    while !stop.load(Ordering::SeqCst) {
                        if Instant::now() >= next {
                            next = Instant::now() + every;
                            reads += 1;
                            // The owner rarely changes: once, then now and then while unknown.
                            let known = matches!(owner, Some(backups::Owner::Known(_)));
                            if owner.is_none() || (!known && reads.is_multiple_of(6)) {
                                match backups::owner(ctx.remote()) {
                                    Ok(o) => owner = Some(o),
                                    Err(e) => {
                                        log::info!("Shared worlds: the owner wasn't read: {e}")
                                    }
                                }
                            }
                            match (backups::request(ctx.remote()), &owner) {
                                (Ok(request), Some(owner)) => {
                                    *locked(&latest) = Some((request, owner.clone()));
                                }
                                (Err(e), _) => {
                                    log::info!("Shared worlds: the load request wasn't read: {e}")
                                }
                                _ => {}
                            }
                        }
                        std::thread::sleep(step);
                    }
                })?
        };
        Ok(Self {
            latest,
            stop,
            thread: Some(thread),
        })
    }

    /// The request as last read (None: there is none) and the owner. None before the first read.
    pub fn latest(&self) -> Option<(Option<backups::Seen>, backups::Owner)> {
        locked(&self.latest).clone()
    }
}

impl Drop for RequestWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Reads a world's lease while someone else hosts it, and says how long it has stood still.
pub struct Watcher {
    latest: Arc<Mutex<Option<(Result<Option<Seen>>, Duration)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    pub fn start(ctx: Arc<Context>) -> Result<Self> {
        Self::start_with(ctx, Watch::new(Instant::now()))
    }

    /// As [`Self::start`], going on from `watch` (what the world list saw of the lease on this
    /// device's clock), so a host that already stood still isn't waited for from the start.
    pub fn start_with(ctx: Arc<Context>, watch: Watch) -> Result<Self> {
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (latest, stop) = (latest.clone(), stop.clone());
            std::thread::Builder::new()
                .name("shared-worlds-watch".into())
                .spawn(move || {
                    let mut watch = watch;
                    let step = Duration::from_millis(50).min(ctx.timing.poll);
                    let mut next = Instant::now();
                    while !stop.load(Ordering::SeqCst) {
                        if Instant::now() >= next {
                            let read = ctx.status();
                            let quiet = match &read {
                                Ok(Some(seen)) => watch.observe_seen(seen, Instant::now()),
                                _ => Duration::ZERO,
                            };
                            // While the world changes hands (or its host is away, or quiet),
                            // three times as often: the next host's code is up the moment it is.
                            let changing = match &read {
                                Ok(Some(seen)) => {
                                    let l = &seen.lease;
                                    matches!(l.state, State::Handoff | State::Starting)
                                        || l.code.is_none()
                                        || l.away
                                        || l.closing
                                        || quiet >= ctx.timing.heartbeat_of(l)
                                }
                                _ => true,
                            };
                            *locked(&latest) = Some((read, quiet));
                            next = Instant::now()
                                + if changing {
                                    ctx.timing.poll / 3
                                } else {
                                    ctx.timing.poll
                                };
                        }
                        std::thread::sleep(step);
                    }
                })?
        };
        Ok(Self {
            latest,
            stop,
            thread: Some(thread),
        })
    }

    /// The lease as last read, and how long it has stood still. None before the first read.
    pub fn latest(&self) -> Option<(Result<Option<Seen>>, Duration)> {
        let latest = locked(&self.latest);
        latest.as_ref().map(|(read, quiet)| {
            let read = match read {
                Ok(seen) => Ok(seen.clone()),
                Err(e) => Err(Error::Store(e.to_string())),
            };
            (read, *quiet)
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        codec::hex,
        s3::Bucket,
        scan::tests::{Scratch, bytes},
        store::MemoryStore,
        sync::tests::go,
    };
    use std::collections::BTreeMap;

    const FAST: Timing = Timing {
        heartbeat: Duration::from_millis(150),
        self_fence: Duration::from_millis(400),
        dead: Duration::from_millis(900),
        turn: Duration::from_millis(100),
        handoff_wait: Duration::from_millis(300),
        poll: Duration::from_millis(30),
        legacy_heartbeat: Duration::from_millis(150),
        legacy_dead: Duration::from_millis(900),
    };

    fn invite() -> WorldInvite {
        WorldInvite {
            bucket: Bucket {
                endpoint: "https://example.r2.cloudflarestorage.com".into(),
                bucket: "b".into(),
                region: "auto".into(),
                access_key: "k".into(),
                secret_key: "s".into(),
            },
            world: "0123456789abcdef".into(),
            name: "Island".into(),
        }
    }

    fn member(n: u32, reachable: bool) -> Member {
        Member {
            uuid: format!("{n:032x}"),
            name: format!("Player{n}"),
            reachable,
        }
    }

    struct Device {
        scratch: Scratch,
        ctx: Arc<Context>,
    }

    impl Device {
        fn new(store: &Arc<MemoryStore>, n: u32) -> Self {
            let scratch = Scratch::new(&format!("dev{n}"));
            let mut ctx = Context::new(
                store.clone(),
                &invite(),
                scratch.path("saves/Island"),
                member(n, n.is_multiple_of(2)),
                format!("device{n}"),
            );
            ctx.timing = FAST;
            Self {
                scratch,
                ctx: Arc::new(ctx),
            }
        }
        fn world(&self) -> PathBuf {
            self.ctx.folder.clone()
        }
        fn write(&self, path: &str, data: &[u8]) {
            let to = under(&self.world(), path);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::write(to, data).unwrap();
        }
        fn read(&self, path: &str) -> Vec<u8> {
            fs::read(under(&self.world(), path)).unwrap()
        }
        fn tree(&self) -> BTreeMap<String, Vec<u8>> {
            walk(&self.world())
                .unwrap()
                .into_iter()
                .map(|f| {
                    let data = fs::read(under(&self.world(), &f.path)).unwrap();
                    (f.path, data)
                })
                .collect()
        }
        fn host(&self, start: Start) -> Result<Hosting> {
            let recovered = self.scratch.path("saves/Island (recovered)");
            host(self.ctx.clone(), start, &move || recovered.clone(), &mut go)
        }
    }

    fn new_world(store: &Arc<MemoryStore>) -> Device {
        let alex = Device::new(store, 1);
        alex.write("level.dat", b"level 1");
        alex.write(
            "dimensions/minecraft/overworld/region/r.0.0.mca",
            &bytes(1, 300_000),
        );
        alex.write("players/data/alex.dat", b"alex");
        create(&alex.ctx, "Island", &mut go).unwrap();
        alex
    }

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "waited too long for {what}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn checkpoint(hosting: &Hosting) -> GenId {
        assert!(hosting.capture(&|| true).unwrap());
        let mut outcome = None;
        wait_for("the upload", || {
            outcome = hosting.poll();
            outcome.is_some()
        });
        outcome.unwrap().unwrap()
    }

    /// The owner's choice of minutes between backups reaches whoever hosts: at the start, and
    /// while hosting (read again in a while, or at once on the owner's own game).
    #[test]
    fn the_host_keeps_backups_at_the_owners_pace() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        let remote = alex.ctx.remote();
        backups::record_owner(remote, &alex.ctx.me).unwrap();
        backups::set_every(remote, &alex.ctx.me, 15).unwrap();
        let hosting = steve.host(Start::Free).unwrap();
        assert_eq!(hosting.backup_every(), 15);
        assert!(!hosting.backup_every_changed());
        // Chosen again by the owner elsewhere: the host follows within its next read.
        backups::set_every(remote, &alex.ctx.me, 2).unwrap();
        wait_for("the host to read the new pace", || hosting.backup_every() == 2);
        assert!(hosting.backup_every_changed());
        assert!(!hosting.backup_every_changed());
        // On the host's own game: at once.
        hosting.set_backup_every(60);
        assert_eq!(hosting.backup_every(), 60);
        assert!(hosting.backup_every_changed());
        hosting.leave(None, &mut go).unwrap();
    }

    /// The game said it ran the world, then stopped saying so (it hung, or lost the world
    /// without letting it go): the heartbeat stops, the host freezes, and the next player may
    /// take over; once the game is back, so is the heartbeat. Away, or leaving, it goes on.
    #[test]
    fn a_host_whose_game_stopped_running_the_world_stops_its_heartbeat() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        let hosting = alex.host(Start::Free).unwrap();
        hosting.open("CODE", None);
        let leases = alex.ctx.leases();
        let beat = || leases.read().unwrap().unwrap().lease.beat;
        // No word from the game yet (getting the world ready): the heartbeat goes on.
        let first = beat();
        wait_for("a heartbeat without the game's word", || beat() > first);
        // The game runs the world: every "frame", for a few heartbeats.
        let start = Instant::now();
        while start.elapsed() < FAST.heartbeat * 3 {
            hosting.alive();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!hosting.must_freeze());
        // It stops: the heartbeat stops (after the one already under way), the host freezes.
        std::thread::sleep(FAST.heartbeat * 2 + FAST.heartbeat / 2);
        let stood = beat();
        std::thread::sleep(FAST.heartbeat * 3);
        assert_eq!(beat(), stood, "the heartbeat went on without the game");
        wait_for("the host to freeze", || hosting.must_freeze());
        // Watched from the start by the next player, the world is theirs to take.
        let mut watch = Watch::new(Instant::now());
        wait_for("the next player to take over", || {
            let seen = leases.read().unwrap().unwrap();
            let quiet = watch.observe_seen(&seen, Instant::now());
            next_step(
                Some(&seen.lease),
                quiet,
                &steve.ctx.me.uuid,
                &steve.ctx.device,
                &FAST,
            ) == Next::Host(Claim::Takeover)
        });
        // The game is back before anyone took it: so is the heartbeat, and the host unfreezes.
        let back = Instant::now();
        let before = beat();
        while back.elapsed() < FAST.heartbeat * 3 {
            hosting.alive();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(beat() > before);
        assert!(!hosting.must_freeze());
        // Away from the screen, no frames: the heartbeat goes on (the away flag says it).
        hosting.set_away(true);
        let away = beat();
        wait_for("a heartbeat while away", || beat() > away + 1);
        hosting.set_away(false);
        // Leaving with no frames at all: the last copy goes in.
        std::thread::sleep(FAST.heartbeat * 3);
        hosting.leave(None, &mut go).unwrap();
        assert_eq!(leases.read().unwrap().unwrap().lease.state, State::Closed);
    }

    /// The storage's own clock says how long the lease has stood still: a player who looks
    /// for the first time long after its host's game stopped isn't made to wait it out again.
    #[test]
    fn a_lease_long_still_by_the_storages_clock_is_seen_still_at_once() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        let hosting = alex.host(Start::Free).unwrap();
        hosting.open("CODE", None);
        wait_for("the code in the lease", || {
            alex.ctx
                .leases()
                .read()
                .unwrap()
                .unwrap()
                .lease
                .code
                .is_some()
        });
        // The game ends without a word (its heartbeat with it).
        hosting.abandon();
        drop(hosting);
        store.backdate(&alex.ctx.lease_key, Duration::from_secs(3600));
        let seen = alex.ctx.leases().read().unwrap().unwrap();
        assert!(seen.age.unwrap() >= Duration::from_secs(3600));
        // A first look: nobody is shown playing it, and it may be taken at once.
        let mut watch = Watch::new(Instant::now());
        let quiet = watch.observe_seen(&seen, Instant::now());
        assert!(FAST.silent(&seen.lease, quiet));
        assert_eq!(
            next_step(
                Some(&seen.lease),
                quiet,
                &steve.ctx.me.uuid,
                &steve.ctx.device,
                &FAST
            ),
            Next::Host(Claim::Takeover)
        );
        // Written again just now: as still as this device saw it, no more.
        let mut held = alex
            .ctx
            .leases()
            .claim(Some(&seen), &steve.ctx.me, "x", Claim::Takeover)
            .unwrap();
        held.heartbeat(std::slice::from_ref(&steve.ctx.me)).unwrap();
        let fresh = alex.ctx.leases().read().unwrap().unwrap();
        assert!(fresh.age.unwrap() < Duration::from_secs(5));
        let mut watch = Watch::new(Instant::now());
        assert!(!FAST.silent(&fresh.lease, watch.observe_seen(&fresh, Instant::now())));
    }

    /// A game on this device held the world under another account (signed in then, offline
    /// now) and stopped: nobody is playing it, and this game takes it over at once, the old
    /// progress kept apart; the same account picks it up as it was.
    #[test]
    fn a_leftover_on_this_device_is_taken_under_any_account() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let hosting = alex.host(Start::Free).unwrap();
        hosting.open("CODE", None);
        wait_for("the code in the lease", || {
            alex.ctx
                .leases()
                .read()
                .unwrap()
                .unwrap()
                .lease
                .code
                .is_some()
        });
        hosting.abandon();
        drop(hosting);
        let seen = alex.ctx.leases().read().unwrap().unwrap();
        let device = &alex.ctx.device;
        assert!(seen.lease.left_here(device));
        assert!(!seen.lease.left_here("another device"));
        let me = &alex.ctx.me.uuid;
        let other = member(7, false).uuid;
        assert_eq!(
            next_step(Some(&seen.lease), Duration::ZERO, me, device, &FAST),
            Next::Host(Claim::Free)
        );
        assert_eq!(
            next_step(Some(&seen.lease), Duration::ZERO, &other, device, &FAST),
            Next::Host(Claim::Takeover)
        );
        // On another device it is still the host's, until its heartbeat has been still long
        // enough.
        assert!(matches!(
            next_step(Some(&seen.lease), Duration::ZERO, &other, "another", &FAST),
            Next::Join { .. }
        ));
    }

    #[test]
    fn a_backup_asked_for_approved_and_loaded_and_the_load_undone() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        let remote = alex.ctx.remote();
        // Alex shared it: Alex is its owner.
        let owner = backups::owner(remote).unwrap();
        assert_eq!(owner, backups::Owner::Known(alex.ctx.me.clone()));
        // Two more copies: "level 2", then "level 3" (with a new file).
        let hosting = alex.host(Start::Free).unwrap();
        alex.write("level.dat", b"level 2");
        let second = checkpoint(&hosting);
        alex.write("level.dat", b"level 3");
        alex.write("data/new.dat", b"new");
        let third = checkpoint(&hosting);
        hosting.leave(None, &mut go).unwrap();
        let newest = alex
            .ctx
            .status()
            .unwrap()
            .unwrap()
            .lease
            .generation
            .unwrap();
        assert!(newest > third);
        let listed = backups::list(remote).unwrap();
        assert!(
            listed
                .iter()
                .any(|b| b.id == second && b.kind == Some(backups::Kind::Checkpoint))
        );
        assert_eq!(listed[0].id, newest);
        assert_eq!(listed[0].kind, Some(backups::Kind::Left));
        // Steve asks for the second copy: it waits for Alex, and nothing loads meanwhile.
        backups::ask(remote, second, &steve.ctx.me, &owner, false).unwrap();
        assert!(matches!(load_now(&steve.ctx), Err(Error::Refused)));
        let seen = backups::request(remote).unwrap().unwrap();
        backups::decide(remote, &seen, &alex.ctx.me, &owner, true).unwrap();
        // Nobody plays it: loaded at once, by whoever carries it out.
        let loaded = load_now(&steve.ctx).unwrap();
        let lease = alex.ctx.status().unwrap().unwrap().lease;
        assert_eq!(lease.state, State::Closed);
        assert_eq!(lease.generation, Some(loaded));
        let record = lease.loaded.clone().unwrap();
        assert_eq!((record.backup, record.replaced), (second, Some(newest)));
        assert_eq!(record.by, steve.ctx.me.name);
        assert_eq!(
            backups::request(remote).unwrap().unwrap().request.state,
            backups::State::Done
        );
        // Carried out once only.
        assert!(matches!(load_now(&steve.ctx), Err(Error::Refused)));
        // The next host plays the backup, whose files are the second copy's.
        let hosting = steve.host(Start::Free).unwrap();
        assert_eq!(steve.read("level.dat"), b"level 2");
        assert!(!steve.world().join("data/new.dat").exists());
        hosting.leave(None, &mut go).unwrap();
        // The load is still known to the next lease, and undone by loading what it replaced.
        let lease = alex.ctx.status().unwrap().unwrap().lease;
        let replaced = lease.loaded.as_ref().unwrap().replaced.unwrap();
        assert_eq!(replaced, newest);
        backups::ask(remote, replaced, &alex.ctx.me, &owner, true).unwrap();
        // The owner's own request needs no answer; a host starting carries it out too.
        let hosting = alex.host(Start::Free).unwrap();
        assert_eq!(alex.read("level.dat"), b"level 3");
        assert_eq!(alex.read("data/new.dat"), b"new");
        let lease = hosting.lease();
        assert_eq!(lease.loaded.as_ref().unwrap().backup, newest);
        // Play goes on from it: a checkpoint is the next copy of this epoch.
        alex.write("level.dat", b"level 4");
        let after = checkpoint(&hosting);
        assert!(after > lease.generation.unwrap());
        hosting.leave(None, &mut go).unwrap();
    }

    #[test]
    fn a_backup_that_is_gone_fails_its_request_and_the_world_goes_on() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let remote = alex.ctx.remote();
        let owner = backups::owner(remote).unwrap();
        let gone = GenId {
            epoch: 1,
            seq: 99,
            day: 20261101,
        };
        backups::ask(remote, gone, &alex.ctx.me, &owner, false).unwrap();
        let hosting = alex.host(Start::Free).unwrap();
        assert_eq!(alex.read("level.dat"), b"level 1");
        let request = backups::request(remote).unwrap().unwrap().request;
        assert_eq!(request.state, backups::State::Failed);
        assert!(request.note.is_some());
        assert!(hosting.lease().loaded.is_none());
        hosting.leave(None, &mut go).unwrap();
    }

    #[test]
    fn a_host_that_picked_its_own_lease_up_loads_an_approved_backup_as_its_next_copy() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let remote = alex.ctx.remote();
        let owner = backups::owner(remote).unwrap();
        let first = alex
            .ctx
            .status()
            .unwrap()
            .unwrap()
            .lease
            .generation
            .unwrap();
        let hosting = alex.host(Start::Free).unwrap();
        alex.write("level.dat", b"level 2");
        checkpoint(&hosting);
        // The game stops without handing the world on; the owner asks for the first copy.
        hosting.abandon();
        backups::ask(remote, first, &alex.ctx.me, &owner, false).unwrap();
        let again = alex.host(Start::Free).unwrap();
        assert_eq!(alex.read("level.dat"), b"level 1");
        let lease = again.lease();
        assert_eq!(lease.loaded.as_ref().unwrap().backup, first);
        assert_eq!(lease.generation.unwrap().epoch, hosting.lease().epoch);
        again.leave(None, &mut go).unwrap();
    }

    #[test]
    fn a_new_world_is_closed_with_its_first_copy() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let lease = alex.ctx.status().unwrap().unwrap().lease;
        assert_eq!((lease.state, lease.playing()), (State::Closed, 0));
        assert_eq!(lease.generation.map(|g| (g.epoch, g.seq)), Some((1, 1)));
        assert_eq!(
            Marker::read(&alex.world()).unwrap().world,
            "0123456789abcdef"
        );
        // The same world can't be made twice; a folder that isn't a world isn't made at all.
        assert!(create(&alex.ctx, "Island", &mut go).is_err());
        let empty_store = Arc::new(MemoryStore::new());
        let empty = Device::new(&empty_store, 5);
        empty.write("notes.txt", b"x");
        assert!(create(&empty.ctx, "Island", &mut go).is_err());
        assert!(
            empty_store.keys().is_empty(),
            "a failed creation leaves nothing behind"
        );
    }

    #[test]
    fn hosting_passes_from_player_to_player_with_every_change() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        // Steve opens it first: his device gets the whole world.
        let hosting = steve.host(Start::Free).unwrap();
        assert_eq!(hosting.fetched.unwrap().replaced, 3);
        assert_eq!(steve.tree(), alex.tree());
        hosting.open("CODE-STEVE", None);
        hosting.set_players(vec![member(2, true), member(1, false)]);
        wait_for("the code and the players in the lease", || {
            let lease = alex.ctx.status().unwrap().unwrap().lease;
            lease.code.is_some() && lease.playing() == 2
        });
        let lease = alex.ctx.status().unwrap().unwrap().lease;
        assert_eq!(lease.playing(), 2);
        assert_eq!(
            next_step(
                Some(&lease),
                Duration::ZERO,
                &member(1, false).uuid,
                "elsewhere",
                &FAST
            ),
            Next::Join {
                code: "CODE-STEVE".into(),
                tunnel: None
            }
        );
        // Play goes on; a checkpoint uploads what changed.
        steve.write("level.dat", b"level 2");
        steve.write("players/data/steve.dat", b"steve");
        let first = checkpoint(&hosting);
        assert_eq!(
            alex.ctx.status().unwrap().unwrap().lease.generation,
            Some(first)
        );
        // Nothing changed: still a copy, and nothing to upload but its list.
        let before = store.bytes();
        checkpoint(&hosting);
        assert!(store.bytes().saturating_sub(before) < 4096);
        // Steve leaves and hands the world to Alex.
        steve.write("level.dat", b"level 3");
        let last = hosting
            .leave(Some(&member(1, false).uuid), &mut go)
            .unwrap();
        let lease = alex.ctx.status().unwrap().unwrap().lease;
        assert_eq!(
            (lease.state, lease.generation),
            (State::Handoff, Some(last))
        );
        assert_eq!(
            next_step(
                Some(&lease),
                Duration::ZERO,
                &member(1, false).uuid,
                "elsewhere",
                &FAST
            ),
            Next::Host(Claim::Free)
        );
        assert_eq!(
            next_step(
                Some(&lease),
                Duration::ZERO,
                &member(3, false).uuid,
                "elsewhere",
                &FAST
            ),
            Next::Wait
        );
        drop(hosting);
        let hosting = alex.host(Start::Free).unwrap();
        assert!(hosting.recovered.is_none());
        assert_eq!(alex.tree(), steve.tree());
        assert_eq!(alex.read("level.dat"), b"level 3");
        assert_eq!(hosting.epoch(), 3);
        hosting.leave(None, &mut go).unwrap();
        let lease = alex.ctx.status().unwrap().unwrap().lease;
        assert_eq!((lease.state, lease.playing()), (State::Closed, 0));
    }

    #[test]
    fn a_checkpoint_the_game_wrote_into_is_given_up() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let hosting = alex.host(Start::Free).unwrap();
        alex.write("level.dat", b"written while copied");
        assert!(!hosting.capture(&|| false).unwrap());
        assert!(!hosting.uploading() && hosting.poll().is_none());
        // The next one takes the file again.
        let id = checkpoint(&hosting);
        let manifest = alex.ctx.remote().manifest(id).unwrap();
        assert_eq!(
            manifest.file("level.dat").unwrap().size,
            b"written while copied".len() as u64
        );
        hosting.abandon();
    }

    #[test]
    fn a_file_that_moves_during_a_checkpoint_gives_it_up_whatever_the_game_says() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let hosting = alex.host(Start::Free).unwrap();
        alex.write("level.dat", b"before the copy");
        // A writer the game didn't count, mid-copy.
        let folder = alex.world();
        let writer = move || {
            fs::write(
                under(&folder, "players/data/alex.dat"),
                b"written during the copy, longer",
            )
            .unwrap();
            true
        };
        assert!(!hosting.capture(&writer).unwrap());
        assert!(!hosting.uploading());
        let id = checkpoint(&hosting);
        let manifest = alex.ctx.remote().manifest(id).unwrap();
        assert_eq!(
            manifest.file("players/data/alex.dat").unwrap().size,
            b"written during the copy, longer".len() as u64
        );
        hosting.abandon();
    }

    #[test]
    fn a_checkpoint_is_what_the_world_was_when_it_was_taken() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let hosting = alex.host(Start::Free).unwrap();
        alex.write("level.dat", b"at the checkpoint");
        assert!(hosting.capture(&|| true).unwrap());
        // The world runs on while the copy uploads.
        alex.write("level.dat", b"after the checkpoint, and longer too");
        let mut outcome = None;
        wait_for("the upload", || {
            outcome = hosting.poll();
            outcome.is_some()
        });
        let id = outcome.unwrap().unwrap();
        let other = Device::new(&store, 4);
        let manifest = other.ctx.remote().manifest(id).unwrap();
        assert_eq!(
            manifest.file("level.dat").unwrap().size,
            b"at the checkpoint".len() as u64
        );
        // The next checkpoint has the newer bytes.
        let next = checkpoint(&hosting);
        let manifest = other.ctx.remote().manifest(next).unwrap();
        assert_eq!(
            manifest.file("level.dat").unwrap().size,
            b"after the checkpoint, and longer too".len() as u64
        );
        hosting.abandon();
    }

    #[test]
    fn a_failed_upload_is_made_again_from_the_same_files() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let hosting = alex.host(Start::Free).unwrap();
        alex.write("players/data/alex.dat", b"alex, later");
        // The storage fails for a while: the upload fails, and so do heartbeats.
        store.fail_before.store(u32::MAX, Ordering::SeqCst);
        assert!(hosting.capture(&|| true).unwrap());
        let mut outcome = None;
        wait_for("the failed upload", || {
            outcome = hosting.poll();
            outcome.is_some()
        });
        assert!(outcome.unwrap().is_err());
        store.fail_before.store(0, Ordering::SeqCst);
        alex.write("level.dat", b"level 2");
        let id = checkpoint(&hosting);
        let fresh = Device::new(&store, 6);
        let manifest = fresh.ctx.remote().manifest(id).unwrap();
        fetch(
            fresh.ctx.remote(),
            &manifest,
            &fresh.ctx.layout(),
            &mut LocalIndex::default(),
            &mut go,
        )
        .unwrap();
        assert_eq!(fresh.tree(), alex.tree());
        hosting.abandon();
    }

    #[test]
    fn a_host_that_stopped_is_taken_over_and_its_own_progress_is_kept() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        let hosting = alex.host(Start::Free).unwrap();
        hosting.open("CODE-ALEX", None);
        hosting.set_players(vec![member(1, false), member(2, true)]);
        alex.write("level.dat", b"saved in the storage");
        checkpoint(&hosting);
        alex.write("level.dat", b"only on alex's device");
        // Alex's game dies: the heartbeat stops, nothing more is uploaded.
        hosting.abandon();
        let watcher = Watcher::start(steve.ctx.clone()).unwrap();
        let mut taken = None;
        wait_for("the takeover", || {
            if let Some((Ok(Some(seen)), quiet)) = watcher.latest()
                && next_step(
                    Some(&seen.lease),
                    quiet,
                    &steve.ctx.me.uuid,
                    &steve.ctx.device,
                    &FAST,
                ) == Next::Host(Claim::Takeover)
            {
                taken = Some(seen);
            }
            taken.is_some()
        });
        let hosting = steve.host(Start::Takeover(taken.unwrap())).unwrap();
        assert_eq!(steve.read("level.dat"), b"saved in the storage");
        steve.write("level.dat", b"steve's");
        hosting.leave(None, &mut go).unwrap();
        // Alex comes back: the newer copy is Steve's, and Alex's own progress is kept apart.
        let again = alex.host(Start::Free).unwrap();
        let kept = again.recovered.clone().unwrap();
        assert_eq!(
            fs::read(kept.join("level.dat")).unwrap(),
            b"only on alex's device"
        );
        assert!(!kept.join(WORK).exists());
        assert_eq!(alex.read("level.dat"), b"steve's");
        again.leave(None, &mut go).unwrap();
    }

    #[test]
    fn a_host_that_was_replaced_can_publish_nothing() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        let hosting = alex.host(Start::Free).unwrap();
        // A heartbeat may land between the read and the takeover: then read again.
        let mut taken = None;
        wait_for("the takeover", || {
            let seen = steve.ctx.status().unwrap().unwrap();
            match steve.host(Start::Takeover(seen)) {
                Ok(hosting) => taken = Some(hosting),
                Err(Error::Refused) => {}
                Err(e) => panic!("{e}"),
            }
            taken.is_some()
        });
        let taken = taken.unwrap();
        alex.write("level.dat", b"too late");
        wait_for("alex to learn", || {
            let _ = hosting.capture(&|| true);
            matches!(hosting.poll(), Some(Err(Error::Fenced))) || hosting.fenced()
        });
        assert!(hosting.must_freeze());
        assert!(matches!(hosting.leave(None, &mut go), Err(Error::Fenced)));
        let committed = steve.ctx.status().unwrap().unwrap().lease.generation;
        let manifest = steve.ctx.remote().manifest(committed.unwrap()).unwrap();
        assert_eq!(
            manifest.file("level.dat").unwrap().size,
            b"level 1".len() as u64
        );
        taken.leave(None, &mut go).unwrap();
        hosting.abandon();
    }

    #[test]
    fn a_host_whose_game_stopped_carries_on_from_its_own_files() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let hosting = alex.host(Start::Free).unwrap();
        hosting.open("CODE", None);
        alex.write("level.dat", b"played, never uploaded");
        hosting.abandon();
        // Back before anyone took it: same epoch, nothing replaced, and the first upload has it.
        let resumed = alex.host(Start::Free).unwrap();
        assert!(resumed.recovered.is_none() && resumed.fetched.is_none());
        assert_eq!(resumed.epoch(), 2);
        assert_eq!(alex.read("level.dat"), b"played, never uploaded");
        let id = checkpoint(&resumed);
        let manifest = alex.ctx.remote().manifest(id).unwrap();
        assert_eq!(
            manifest.file("level.dat").unwrap().size,
            b"played, never uploaded".len() as u64
        );
        resumed.abandon();
    }

    #[test]
    fn a_host_that_cant_reach_the_storage_freezes_in_time() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let hosting = alex.host(Start::Free).unwrap();
        assert!(!hosting.must_freeze());
        store.fail_before.store(u32::MAX, Ordering::SeqCst);
        let lost = Instant::now();
        wait_for("the freeze", || hosting.must_freeze());
        assert!(
            lost.elapsed() < FAST.dead,
            "frozen before anyone may take over"
        );
        store.fail_before.store(0, Ordering::SeqCst);
        wait_for("the thaw", || !hosting.must_freeze());
        hosting.abandon();
    }

    #[test]
    fn two_games_that_open_a_closed_world_together_get_one_host() {
        let store = Arc::new(MemoryStore::new());
        let _alex = new_world(&store);
        let players: Vec<Device> = (2..6).map(|n| Device::new(&store, n)).collect();
        let results: Vec<Result<Hosting>> = std::thread::scope(|scope| {
            let games: Vec<_> = players
                .iter()
                .map(|p| scope.spawn(move || p.host(Start::Free)))
                .collect();
            games.into_iter().map(|g| g.join().unwrap()).collect()
        });
        let hosts: Vec<&Hosting> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
        assert_eq!(hosts.len(), 1);
        assert!(
            results
                .iter()
                .filter_map(|r| r.as_ref().err())
                .all(|e| matches!(e, Error::Refused))
        );
    }

    #[test]
    fn a_guest_keeps_its_copy_close_to_the_newest() {
        let store = Arc::new(MemoryStore::new());
        let alex = new_world(&store);
        let steve = Device::new(&store, 2);
        assert_eq!(prefetch(&steve.ctx, &mut go).unwrap().unwrap().replaced, 3);
        assert!(prefetch(&steve.ctx, &mut go).unwrap().is_none());
        let hosting = alex.host(Start::Free).unwrap();
        alex.write("level.dat", b"level 2");
        checkpoint(&hosting);
        let fetched = prefetch(&steve.ctx, &mut go).unwrap().unwrap();
        assert_eq!(fetched.replaced, 1);
        assert_eq!(steve.read("level.dat"), b"level 2");
        // While the folder is in use it is left alone.
        let _lock = FolderLock::take(&steve.world()).unwrap();
        alex.write("level.dat", b"level 3");
        checkpoint(&hosting);
        assert!(prefetch(&steve.ctx, &mut go).unwrap().is_none());
        hosting.abandon();
    }

    #[test]
    fn the_next_host_is_one_friends_can_reach() {
        let host = member(1, true).uuid;
        assert_eq!(
            successor(&[member(1, true), member(3, false), member(2, true)], &host),
            Some(member(2, true).uuid)
        );
        assert_eq!(
            successor(&[member(1, true), member(3, false)], &host),
            Some(member(3, false).uuid)
        );
        assert_eq!(successor(&[member(1, true)], &host), None);
    }

    #[test]
    fn what_a_waiting_player_does() {
        let me = member(3, false).uuid;
        let lease = |state, code: Option<&str>, successor: Option<u32>| Lease {
            version: 1,
            epoch: 4,
            state,
            host: Some(member(1, true)),
            device: "d".into(),
            successor: successor.map(|n| member(n, false).uuid),
            beat: 0,
            token: hex(b"t"),
            code: code.map(str::to_owned),
            tunnel: false,
            tunnel_at: None,
            closing: false,
            loaded: None,
            beat_ms: None,
            dead_ms: None,
            away: false,
            players: vec![member(1, true), member(2, true), member(3, false)],
            generation: None,
        };
        let at = |l: &Lease, ms: u64| {
            next_step(Some(l), Duration::from_millis(ms), &me, "elsewhere", &FAST)
        };
        let hosting = lease(State::Hosting, Some("C"), None);
        assert_eq!(
            at(&hosting, 0),
            Next::Join {
                code: "C".into(),
                tunnel: None
            }
        );
        // Dead at 900 ms for the first in line; I'm second (100 ms later).
        assert_eq!(
            at(&hosting, 950),
            Next::Join {
                code: "C".into(),
                tunnel: None
            }
        );
        assert_eq!(at(&hosting, 1000), Next::Host(Claim::Takeover));
        // A host behind the public tunnel: the guest learns where it is reached.
        let tunneled = Lease {
            tunnel: true,
            tunnel_at: Some("203.0.113.9:4000".into()),
            ..hosting.clone()
        };
        assert_eq!(
            at(&tunneled, 0),
            Next::Join {
                code: "C".into(),
                tunnel: Some("203.0.113.9:4000".into())
            }
        );
        // An older host's lease says only that it has one: every endpoint is dialled at once.
        let older = Lease {
            tunnel: true,
            tunnel_at: None,
            closing: false,
            loaded: None,
            ..hosting.clone()
        };
        assert_eq!(
            at(&older, 0),
            Next::Join {
                code: "C".into(),
                tunnel: None
            }
        );
        assert_eq!(at(&lease(State::Starting, None, None), 500), Next::Wait);
        let handoff = lease(State::Handoff, None, Some(2));
        assert_eq!(at(&handoff, 350), Next::Wait);
        assert_eq!(at(&handoff, 400), Next::Host(Claim::Takeover));
        assert_eq!(
            at(&lease(State::Handoff, None, Some(3)), 0),
            Next::Host(Claim::Free)
        );
        assert_eq!(
            at(&lease(State::Closed, None, None), 0),
            Next::Host(Claim::Free)
        );
        assert_eq!(
            next_step(None, Duration::ZERO, &me, "elsewhere", &FAST),
            Next::Gone
        );
        assert!(FAST.consistent());
        // The host's own game on its own device, after it stopped (closed, or ended by the
        // system): it takes the world back at once, Hosting or Starting, whatever the code.
        let host = member(1, true).uuid;
        for state in [State::Hosting, State::Starting] {
            assert_eq!(
                next_step(
                    Some(&lease(state, Some("C"), None)),
                    Duration::ZERO,
                    &host,
                    "d",
                    &FAST
                ),
                Next::Host(Claim::Free)
            );
            // The same player on another device waits as anyone does: that one may be playing.
            assert_eq!(
                next_step(
                    Some(&lease(state, None, None)),
                    Duration::ZERO,
                    &host,
                    "other",
                    &FAST
                ),
                Next::Wait
            );
        }
        // A world it handed off or closed isn't its own leftover.
        assert_eq!(
            next_step(
                Some(&lease(State::Handoff, None, Some(2))),
                Duration::ZERO,
                &host,
                "d",
                &FAST
            ),
            Next::Wait
        );
        // Silence: shown after two heartbeats and two reads; taken only at `dead`.
        let paced = Lease {
            beat_ms: Some(150),
            dead_ms: Some(900),
            ..lease(State::Hosting, Some("C"), None)
        };
        assert!(!FAST.silent(&paced, FAST.heartbeat * 2));
        assert!(FAST.silent(&paced, FAST.heartbeat * 2 + FAST.poll * 2));
        assert_eq!(FAST.left_to_wait(&paced, Duration::ZERO, 0), FAST.dead);
        assert_eq!(FAST.left_to_wait(&paced, FAST.dead, 1), FAST.turn);
        assert_eq!(FAST.left_to_wait(&paced, FAST.dead * 2, 0), Duration::ZERO);
        // A lease that doesn't say its host's pace (an older game's) is waited for as long as
        // that game needs, and a host that says it needs longer than this game gets it.
        let older = Lease {
            beat_ms: None,
            dead_ms: None,
            ..paced.clone()
        };
        assert_eq!(FAST.dead_for(&older), FAST.legacy_dead);
        assert_eq!(FAST.heartbeat_of(&older), FAST.legacy_heartbeat);
        let slow = Lease {
            dead_ms: Some(5_000),
            ..paced.clone()
        };
        assert_eq!(FAST.dead_for(&slow), Duration::from_secs(5));
        assert_eq!(
            next_step(
                Some(&slow),
                Duration::from_millis(1_000),
                &me,
                "elsewhere",
                &FAST
            ),
            Next::Join {
                code: "C".into(),
                tunnel: None
            }
        );
        assert_eq!(
            next_step(
                Some(&slow),
                Duration::from_millis(5_100),
                &me,
                "elsewhere",
                &FAST
            ),
            Next::Host(Claim::Takeover)
        );
    }

    #[test]
    fn a_world_copy_leaves_out_the_lock_and_the_working_folder() {
        let from = Scratch::new("copyfrom");
        from.write("level.dat", b"l");
        from.write("session.lock", b"");
        from.write(".shared-world/index.json", b"{}");
        from.write("region/r.0.0.mca", b"r");
        let to = Scratch::new("copyto");
        let target = to.path("copy");
        copy_world(&from.0, &target).unwrap();
        let mut names: Vec<String> = walk(&target).unwrap().into_iter().map(|f| f.path).collect();
        names.sort();
        assert_eq!(names, ["level.dat", "region/r.0.0.mca"]);
        assert!(!target.join("session.lock").exists() && !target.join(WORK).exists());
        assert!(
            copy_world(&from.0, &target).is_err(),
            "never over an existing folder"
        );
    }
}
