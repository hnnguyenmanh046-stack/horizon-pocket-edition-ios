//! Shared Worlds on the title screen: the worlds this player shares with friends, how many are
//! in each now, and starting or joining one; adding a world from a friend's invite; and sharing
//! one of this device's worlds (`crate::shared_worlds` does the work).
use super::{centered, wrapped};
use crate::{
    clipboard::{Clipboard, FieldId, SystemClipboard},
    hud::{HudAtlas, HudBatch, rgba},
    shared_worlds::{self, Entry, Status, Task},
};
use horizon_shared_worlds::{
    backups::{self, Kind, Owner},
    manifest::GenId,
    s3::Bucket,
};
use std::time::Instant;

fn tr(key: &str, english: &str) -> String {
    crate::lang::get_or(&format!("horizon.shared.{key}"), english)
}
fn grey() -> u32 {
    rgba(170, 170, 170, 255)
}

const PLAY: u8 = 200;
const COPY: u8 = 201;
const ADD: u8 = 202;
const REMOVE: u8 = 203;
const BACK: u8 = 204;
const PREVIOUS: u8 = 205;
const NEXT: u8 = 206;
const SHARE: u8 = 207;
const WORLD_BACK: u8 = 209;
const ROW: u8 = 210;
const ADD_CONFIRM: u8 = 220;
const ADD_CANCEL: u8 = 221;
const PASTE: u8 = 222;
const PICK_ROW: u8 = 230;
const PICK_PREVIOUS: u8 = 236;
const PICK_NEXT: u8 = 237;
const PICK_CANCEL: u8 = 239;
const STORAGE_SHARE: u8 = 240;
const STORAGE_CANCEL: u8 = 241;
const STORAGE_HELP: u8 = 242;
const TASK_CANCEL: u8 = 250;
const TASK_OK: u8 = 251;
const REMOVE_CONFIRM: u8 = 252;
const REMOVE_CANCEL: u8 = 253;
const BACKUPS: u8 = 208;
const HELP_BACK: u8 = 216;
const BACKUP_ROW: u8 = 223;
const BACKUP_PREVIOUS: u8 = 229;
const BACKUP_NEXT: u8 = 238;
const BACKUP_BACK: u8 = 243;
const BACKUP_EVERY: u8 = 217;
const APPROVE: u8 = 244;
const DECLINE: u8 = 245;
const WITHDRAW: u8 = 246;
const UNDO: u8 = 247;
const CLAIM: u8 = 248;
const LOAD_CONFIRM: u8 = 249;
const LOAD_CANCEL: u8 = 254;
const HELP_OPEN: u8 = 255;

/// Where Open Cloudflare goes: R2's page, after signing in or up.
pub const CLOUDFLARE_URL: &str = "https://dash.cloudflare.com/?to=/:account/r2/overview";

const INVITE_FIELD: FieldId = ("shared.invite", 0);
const STORAGE_FIELDS: [(&str, &str, usize); 4] = [
    ("storage_address", "S3 endpoint (or account ID)", 256),
    ("bucket", "Bucket name", 63),
    ("access_key", "Access key ID", 128),
    ("secret_key", "Secret access key", 256),
];

#[derive(Clone, Debug, Default, PartialEq)]
enum Page {
    #[default]
    List,
    World(String),
    Remove(String),
    Add,
    Pick,
    Storage {
        folder: String,
        name: String,
    },
    /// How to make the storage, from the Storage page.
    Help {
        folder: String,
        name: String,
    },
    /// A world's backups.
    Backups(String),
    /// Loading one (`undo`: the copy the last load replaced).
    Load {
        world: String,
        backup: GenId,
        undo: bool,
    },
}

/// The few steps of making the storage, for the Help page.
pub fn help_text() -> String {
    tr(
        "help_steps",
        "Once, on this phone, about 5 minutes. Your friends need none of this: only the invite.\n\n1. Tap Open Cloudflare and make a free account.\n2. In R2 Object Storage, turn R2 on (it asks for a card: the free 10 GB is never charged), then Create bucket with any name.\n3. In R2, Manage API tokens, Create API token: Object Read & Write, then Create.\n4. Copy its S3 endpoint, Access Key ID and Secret Access Key, and Paste each here with the bucket's name. Share World.",
    )
}

/// What is pasted as the storage address: Cloudflare's S3 endpoint, an endpoint with the
/// bucket's name after it (both are taken), or the account ID alone (the endpoint is made from
/// it). (address, bucket).
fn storage_address(text: &str) -> (String, Option<String>) {
    let text = text.trim();
    if text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return (
            format!(
                "https://{}.r2.cloudflarestorage.com",
                text.to_ascii_lowercase()
            ),
            None,
        );
    }
    if let Some(rest) = text.strip_prefix("https://")
        && let Some((host, path)) = rest.split_once('/')
    {
        let bucket = path.trim_matches('/');
        let bucket = (!bucket.is_empty() && !bucket.contains('/')).then(|| bucket.to_owned());
        return (format!("https://{host}"), bucket);
    }
    (text.to_owned(), None)
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
}

/// How many backups a page shows.
fn backup_rows(h: f32) -> usize {
    (((h - BACKUP_TOP - 68.0) / 24.0).floor() as usize).clamp(1, 6)
}

/// A backup's row: when, who, what it was.
fn backup_label(backup: &backups::Backup) -> String {
    let mut label = shared_worlds::when(backup);
    if let Some(by) = &backup.by {
        label.push_str(&format!(" - {by}"));
    }
    match backup.kind {
        Some(Kind::Left) => label.push_str(&format!(" ({})", tr("kind_left", "left"))),
        Some(Kind::First) => label.push_str(&format!(" ({})", tr("kind_first", "shared"))),
        Some(Kind::Loaded(_)) => label.push_str(&format!(" ({})", tr("kind_loaded", "a load"))),
        _ => {}
    }
    label
}

/// The Backups page's lines: the request, the last load, the owner, the storage.
fn backup_lines(details: &shared_worlds::Details) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(e) = &details.error {
        out.push(e.clone());
    }
    if !details.read {
        if details.error.is_none() {
            out.push(tr("reading", "Reading..."));
        }
        return out;
    }
    let owner = details
        .owner
        .as_ref()
        .and_then(|o| o.name().map(str::to_owned))
        .unwrap_or_default();
    match details.request.as_ref().map(|s| &s.request) {
        Some(r) if r.state == backups::State::Asked => out.push(
            tr(
                "asked_load",
                "%a asks to load the backup from %t. %o decides.",
            )
            .replacen("%a", &r.by.name, 1)
            .replacen("%t", &shared_worlds::when_id(details, r.backup), 1)
            .replacen("%o", &owner, 1),
        ),
        Some(r) if r.state == backups::State::Approved => out.push(
            tr(
                "approved_load",
                "Approved: the backup from %t loads as the world next opens.",
            )
            .replacen("%t", &shared_worlds::when_id(details, r.backup), 1),
        ),
        Some(r) if r.state == backups::State::Failed => {
            let note: String = r.note.as_deref().unwrap_or("").chars().take(60).collect();
            out.push(tr("failed_load", "Couldn't load it: %s").replacen("%s", &note, 1));
        }
        _ => {
            if let Some(loaded) = &details.loaded {
                out.push(
                    tr("last_load", "Last load: the backup from %t, asked by %a.")
                        .replacen("%t", &shared_worlds::when_id(details, loaded.backup), 1)
                        .replacen("%a", &loaded.by, 1),
                );
            }
        }
    }
    match &details.owner {
        Some(Owner::Unknown) => out.push(tr(
            "no_owner",
            "No owner recorded. Whoever shared it: tap I Shared It.",
        )),
        Some(_) if details.owner_is_me() => out.push(tr(
            "you_own",
            "You shared it: loads others ask for need your approval.",
        )),
        Some(_) => {
            out.push(tr("owner_is", "%s shared it and approves loads.").replacen("%s", &owner, 1))
        }
        None => {}
    }
    out.push(every_line(details));
    out
}

/// How often a backup is kept, and what that means: the most a host's phone dying can lose.
fn every_line(details: &shared_worlds::Details) -> String {
    let line = if details.owner_is_me() {
        tr(
            "every_yours",
            "A backup every %d minutes while played, the most a crash can lose. Tap Backups to change it.",
        )
    } else {
        tr(
            "every_theirs",
            "A backup every %d minutes while played, the most a crash can lose.",
        )
    };
    line.replacen("%d", &details.every.to_string(), 1)
}

/// The pace after `minutes` in the owner's choices, round again after the last.
fn next_every(minutes: u32) -> u32 {
    let at = backups::EVERY.iter().position(|m| *m == minutes);
    backups::EVERY[at.map_or(0, |i| (i + 1) % backups::EVERY.len())]
}

/// The storage the world and its bucket use, one short line (the oldest backups go first when
/// the bucket nears its limit).
fn usage_line(details: &shared_worlds::Details) -> Option<String> {
    details.usage.map(|(all, world)| {
        tr("usage", "This world %w, the bucket %a of 8 GB.")
            .replacen("%w", &gigabytes(world), 1)
            .replacen("%a", &gigabytes(all), 1)
    })
}

/// Where the Backups page's rows begin: its lines stop above.
const BACKUP_TOP: f32 = 96.0;

/// What a click asks of the title.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// Back to the main menu.
    Close,
    /// Start or join the world with this id.
    Play(String),
}

#[derive(Clone, Default)]
pub struct Screen {
    page: Page,
    offset: usize,
    invite: String,
    /// The storage fields, in [`STORAGE_FIELDS`] order.
    storage: [String; 4],
    focus: usize,
    /// One line under the page's buttons: what the last action did.
    notice: Option<String>,
    copied: Option<Instant>,
    /// This device's singleplayer worlds, (folder, name), for sharing one.
    local: Vec<(String, String)>,
}

impl std::fmt::Debug for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedWorldsScreen")
            .field("page", &self.page)
            .field("storage", &"<hidden>")
            .finish()
    }
}

fn rows(h: f32) -> usize {
    (((h - 120.0) / 24.0).floor() as usize).clamp(1, 6)
}

/// A row's text: the world's name and who is in it.
fn summary(entry: &Entry) -> String {
    format!("{}  -  {}", entry.name, status_line(&entry.status))
}

fn status_line(status: &Status) -> String {
    match status {
        Status::Unknown => tr("reading", "Reading..."),
        Status::Failed(_) => tr("unreachable", "Storage unreachable"),
        Status::Known { playing: 0, .. } => tr("nobody", "Nobody playing"),
        Status::Known {
            away: true,
            silent: false,
            host,
            ..
        } => tr("away", "%s's game is in the background").replacen(
            "%s",
            host.as_deref().unwrap_or(""),
            1,
        ),
        Status::Known {
            silent: true, host, ..
        } => tr("silent", "%s's game isn't answering").replacen(
            "%s",
            host.as_deref().unwrap_or(""),
            1,
        ),
        Status::Known {
            playing,
            host,
            open,
            ..
        } => {
            let host = host.clone().unwrap_or_default();
            if *open {
                tr("playing_host", "%d playing, hosted by %s")
                    .replacen("%d", &playing.to_string(), 1)
                    .replacen("%s", &host, 1)
            } else {
                tr("starting", "%s is opening it").replacen("%s", &host, 1)
            }
        }
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_000_000.0)
}

impl Screen {
    pub fn new() -> Self {
        shared_worlds::refresh(true);
        Self::default()
    }

    fn entry(&self, id: &str) -> Option<Entry> {
        shared_worlds::entries().into_iter().find(|e| e.id == id)
    }

    fn storage_page(&self) -> bool {
        matches!(self.page, Page::Storage { .. })
    }

    /// The focused text field, for typing, the caret, copy and paste.
    pub(super) fn field(&mut self) -> Option<(FieldId, &mut String, usize)> {
        if task_shown() {
            return None;
        }
        match self.page {
            Page::Add => Some((INVITE_FIELD, &mut self.invite, 4096)),
            Page::Storage { .. } => {
                let i = self.focus.min(3);
                Some((
                    ("shared.storage", i),
                    &mut self.storage[i],
                    STORAGE_FIELDS[i].2,
                ))
            }
            _ => None,
        }
    }

    pub(super) fn field_rect(&self, w: f32) -> Option<[f32; 4]> {
        let x = (w / 2.0).floor() - 150.0;
        match self.page {
            Page::Add => Some([x, 60.0, 300.0, 20.0]),
            Page::Storage { .. } => Some([x, 50.0 + 36.0 * self.focus.min(3) as f32, 300.0, 20.0]),
            _ => None,
        }
    }

    /// The secret access key is never shown.
    pub(super) fn password_masked(&self) -> bool {
        self.storage_page() && self.focus == 3
    }

    pub(super) fn focus_at(&mut self, (x, y): (f32, f32), w: f32) {
        if !self.storage_page() || (x - w / 2.0).abs() > 150.0 {
            return;
        }
        for i in 0..4 {
            let top = 50.0 + 36.0 * i as f32;
            if (top..top + 20.0).contains(&y) {
                self.focus = i;
            }
        }
    }

    pub(super) fn tab(&mut self, reverse: bool) -> bool {
        if !self.storage_page() {
            return false;
        }
        self.focus = (self.focus + if reverse { 3 } else { 1 }) % 4;
        true
    }

    pub(super) fn submit(&mut self, h: f32) -> bool {
        match self.page {
            Page::Add if !self.invite.trim().is_empty() => {
                self.click(ADD_CONFIRM, h, std::path::Path::new(""));
                true
            }
            Page::Storage { .. } if self.storage_ready() => {
                self.click(STORAGE_SHARE, h, std::path::Path::new(""));
                true
            }
            Page::Storage { .. } => self.tab(false),
            _ => false,
        }
    }

    /// Escape: back one page; false on the list, which leaves the screen.
    pub(super) fn back(&mut self) -> bool {
        if matches!(shared_worlds::task(), Task::Working { .. }) {
            shared_worlds::cancel();
            return true;
        }
        if task_shown() {
            shared_worlds::dismiss();
            return true;
        }
        if self.page == Page::List {
            return false;
        }
        self.leave_page();
        true
    }

    fn leave_page(&mut self) {
        self.page = match std::mem::take(&mut self.page) {
            Page::Remove(id) | Page::Backups(id) => Page::World(id),
            Page::Storage { .. } => Page::Pick,
            Page::Help { folder, name } => Page::Storage { folder, name },
            Page::Load { world, .. } => Page::Backups(world),
            _ => Page::List,
        };
        self.notice = None;
        self.invite.clear();
        if !self.storage_page() {
            for field in &mut self.storage {
                field.clear();
            }
        }
    }

    fn storage_ready(&self) -> bool {
        self.storage.iter().all(|f| !f.trim().is_empty())
    }

    fn bucket(&self) -> Bucket {
        Bucket {
            endpoint: self.storage[0].trim().to_owned(),
            bucket: self.storage[1].trim().to_owned(),
            region: "auto".into(),
            access_key: self.storage[2].trim().to_owned(),
            secret_key: self.storage[3].trim().to_owned(),
        }
    }

    pub(super) fn draw(
        &self,
        atlas: &HudAtlas,
        batch: &mut HudBatch,
        caret: &mut crate::ui::Caret,
        started: Instant,
    ) {
        let (w, h) = (batch.width.ceil(), batch.height.ceil());
        crate::ui::menu_background(batch, atlas, 0.0, 0.0, w, h, false);
        centered(atlas, batch, &tr("title", "Shared Worlds"), 20.0, u32::MAX);
        let width = (w - 50.0).min(320.0);
        let lines = |batch: &mut HudBatch, text: &str, mut y: f32, color: u32| {
            for line in wrapped(atlas, text, width) {
                centered(atlas, batch, &line, y, color);
                y += 10.0;
            }
            y
        };
        match shared_worlds::task() {
            Task::Working { what, done, total } => {
                let y = lines(batch, &what, 60.0, u32::MAX);
                let detail = if total > 0 {
                    format!(
                        "{} / {} MB ({}%)",
                        megabytes(done),
                        megabytes(total),
                        done * 100 / total.max(1)
                    )
                } else {
                    ".".repeat((started.elapsed().as_millis() / 400 % 4) as usize + 1)
                };
                centered(atlas, batch, &detail, y + 6.0, grey());
                return;
            }
            Task::Failed(message) | Task::Done(message) => {
                lines(batch, &message, 60.0, u32::MAX);
                return;
            }
            Task::Idle => {}
        }
        match &self.page {
            Page::List => {
                let entries = shared_worlds::entries();
                let message = if entries.is_empty() {
                    tr(
                        "empty",
                        "No shared worlds yet. Share one of your worlds, or add one with the invite a friend sent you.",
                    )
                } else {
                    tr(
                        "intro",
                        "Whoever opens a world while nobody is playing hosts it.",
                    )
                };
                lines(batch, &message, 36.0, grey());
            }
            Page::World(id) => {
                let Some(entry) = self.entry(id) else {
                    return;
                };
                let mut y = lines(batch, &entry.name, 40.0, u32::MAX) + 4.0;
                y = lines(batch, &status_line(&entry.status), y, grey());
                match &entry.status {
                    Status::Failed(e) => {
                        lines(batch, e, y + 4.0, grey());
                    }
                    Status::Known { silent: true, .. } => {
                        lines(
                            batch,
                            &tr(
                                "silent_note",
                                "If it doesn't come back, Start takes the world from its last copy (at most 5 minutes old) once it has been silent for about half a minute.",
                            ),
                            y + 4.0,
                            grey(),
                        );
                    }
                    Status::Known { tunnel: true, .. } => {
                        lines(
                            batch,
                            &tr(
                                "tunnel_note",
                                "Friends who can't connect to this host directly go through a free public tunnel (bore.pub): it may be slower.",
                            ),
                            y + 4.0,
                            grey(),
                        );
                    }
                    _ => {}
                }
            }
            Page::Help { .. } => {
                let mut y = 34.0;
                for paragraph in help_text().split('\n') {
                    if paragraph.is_empty() {
                        y += 4.0;
                    } else {
                        y = lines(batch, paragraph, y, u32::MAX);
                    }
                }
            }
            Page::Backups(id) => {
                let details = shared_worlds::details(id);
                if details.read_at.is_none_or(|t| t.elapsed().as_secs() >= 10) {
                    shared_worlds::read_details(id);
                }
                let name = self.entry(id).map(|e| e.name).unwrap_or_default();
                let mut y = lines(
                    batch,
                    &tr("backups_of", "Backups of %s").replacen("%s", &name, 1),
                    34.0,
                    u32::MAX,
                );
                // As many lines as fit above the rows.
                for line in backup_lines(&details) {
                    for part in wrapped(atlas, &line, width) {
                        if y + 10.0 > BACKUP_TOP - 2.0 {
                            break;
                        }
                        centered(atlas, batch, &part, y, grey());
                        y += 10.0;
                    }
                }
                if let Some(usage) = usage_line(&details) {
                    centered(atlas, batch, &usage, h - 64.0, grey());
                }
                if details.read && details.backups.len() < 2 {
                    lines(
                        batch,
                        &tr(
                            "no_backups",
                            "No backups yet: one is kept every %d minutes while the world is played.",
                        )
                        .replacen("%d", &details.every.to_string(), 1),
                        y + 4.0,
                        grey(),
                    );
                }
            }
            Page::Load {
                world,
                backup,
                undo,
            } => {
                let details = shared_worlds::details(world);
                let when = shared_worlds::when_id(&details, *backup);
                let text = if *undo {
                    tr(
                        "undo_confirm",
                        "Undo the last load? The world goes back to how it was just before it (%t). What was played since stays as a backup.",
                    )
                } else {
                    tr(
                        "load_confirm",
                        "Load the backup from %t? The world goes back to how it was then, for everyone. What was played since stays as a backup, and the load can be undone.",
                    )
                }
                .replacen("%t", &when, 1);
                let mut y = lines(batch, &text, 44.0, u32::MAX) + 6.0;
                let note = if details.owner_is_me() {
                    tr(
                        "you_approve",
                        "You shared this world: it loads without asking anyone.",
                    )
                } else {
                    tr(
                        "owner_approves",
                        "%s, who shared it, has to approve it first.",
                    )
                    .replacen(
                        "%s",
                        details
                            .owner
                            .as_ref()
                            .and_then(|o| o.name())
                            .unwrap_or("The owner"),
                        1,
                    )
                };
                y = lines(batch, &note, y, grey());
                if details.hosted {
                    lines(
                        batch,
                        &tr(
                            "load_while_played",
                            "The world is being played: once approved, the host's game closes it for a moment and opens it again from the backup.",
                        ),
                        y + 4.0,
                        grey(),
                    );
                }
            }
            Page::Remove(id) => {
                let name = self.entry(id).map(|e| e.name).unwrap_or_default();
                lines(
                    batch,
                    &tr(
                        "remove_confirm",
                        "Take %s off this device? It stays in its storage for your friends, and your copy becomes a normal singleplayer world.",
                    )
                    .replacen("%s", &name, 1),
                    50.0,
                    u32::MAX,
                );
            }
            Page::Add => {
                centered(
                    atlas,
                    batch,
                    &tr("paste_invite", "Paste the world invite a friend sent you"),
                    46.0,
                    grey(),
                );
                let rect = [(w / 2.0).floor() - 150.0, 60.0, 300.0, 20.0];
                caret.draw(
                    batch,
                    atlas,
                    (INVITE_FIELD, &self.invite, 4096),
                    rect,
                    true,
                    started,
                );
            }
            Page::Pick => {
                let message = if self.local.is_empty() {
                    tr("no_local", "You have no singleplayer world to share.")
                } else {
                    tr(
                        "pick",
                        "Which world? It moves to this list, and stays on this device.",
                    )
                };
                lines(batch, &message, 36.0, grey());
            }
            Page::Storage { name, .. } => {
                for (i, (key, english, max)) in STORAGE_FIELDS.iter().enumerate() {
                    let y = 50.0 + 36.0 * i as f32;
                    centered(atlas, batch, &tr(key, english), y - 11.0, grey());
                    let field = (("shared.storage", i), self.storage[i].as_str(), *max);
                    let rect = [(w / 2.0).floor() - 150.0, y, 300.0, 20.0];
                    if i == 3 {
                        caret.draw_password(batch, atlas, field, rect, self.focus == i, started);
                    } else {
                        caret.draw(batch, atlas, field, rect, self.focus == i, started);
                    }
                }
                lines(
                    batch,
                    &tr("sharing", "Sharing %s").replacen("%s", name, 1),
                    50.0 + 36.0 * 4.0 - 6.0,
                    grey(),
                );
            }
        }
        if let Some(notice) = self.notice.clone().or_else(shared_worlds::flashed) {
            lines(batch, &notice, h - 64.0, u32::MAX);
        }
    }

    pub(super) fn buttons(&self, w: f32, h: f32) -> Vec<(f32, f32, f32, String, bool, u8)> {
        let cx = (w / 2.0).floor();
        let mut out = Vec::new();
        let mut push = |x: f32, y: f32, bw: f32, label: String, enabled: bool, id: u8| {
            out.push((x, y, bw, label, enabled, id));
        };
        match shared_worlds::task() {
            Task::Working { .. } => {
                push(
                    cx - 100.0,
                    h - 27.0,
                    200.0,
                    tr("cancel", "Cancel"),
                    true,
                    TASK_CANCEL,
                );
                return out;
            }
            Task::Failed(_) | Task::Done(_) => {
                push(cx - 100.0, h - 27.0, 200.0, "OK".into(), true, TASK_OK);
                return out;
            }
            Task::Idle => {}
        }
        match &self.page {
            Page::List => {
                let entries = shared_worlds::entries();
                let n = rows(h);
                let offset = self.offset.min(entries.len().saturating_sub(1) / n * n);
                for (i, entry) in entries.iter().skip(offset).take(n).enumerate() {
                    push(
                        cx - 150.0,
                        56.0 + 24.0 * i as f32,
                        300.0,
                        summary(entry),
                        true,
                        ROW + i as u8,
                    );
                }
                if entries.len() > n {
                    push(cx - 176.0, 56.0, 20.0, "<".into(), offset > 0, PREVIOUS);
                    push(
                        cx + 156.0,
                        56.0,
                        20.0,
                        ">".into(),
                        offset + n < entries.len(),
                        NEXT,
                    );
                }
                push(
                    cx - 154.0,
                    h - 52.0,
                    150.0,
                    tr("share", "Share a World..."),
                    true,
                    SHARE,
                );
                push(
                    cx + 4.0,
                    h - 52.0,
                    150.0,
                    tr("add", "Add with Invite..."),
                    true,
                    ADD,
                );
                push(cx - 100.0, h - 27.0, 200.0, tr("back", "Back"), true, BACK);
            }
            Page::World(id) => {
                let status = self.entry(id).map(|e| e.status);
                // Never greyed out: what the button does is decided from the lease as it is
                // when pressed (join the host, wait for it, or take the world once its host is
                // surely gone), so a list that is behind never keeps anyone out.
                let label = match &status {
                    Some(Status::Known { playing: 0, .. }) => tr("start", "Start"),
                    // Its host stopped answering: Start waits until it is surely gone, then
                    // takes the world from its last copy (or joins it, if it comes back).
                    Some(Status::Known { silent: true, .. } | Status::Known { away: true, .. }) => {
                        tr("start", "Start")
                    }
                    // Open, or opening (joined once it is).
                    Some(Status::Known {
                        host: Some(host), ..
                    }) => tr("join", "Join %s").replacen("%s", host, 1),
                    _ => tr("play", "Play"),
                };
                push(cx - 100.0, 110.0, 200.0, label, true, PLAY);
                let copied = self.copied.is_some_and(|t| t.elapsed().as_secs() < 2);
                let copy = if copied {
                    tr("copied", "Copied")
                } else {
                    tr("copy_invite", "Copy World Invite")
                };
                push(cx - 100.0, 134.0, 200.0, copy, true, COPY);
                push(
                    cx - 100.0,
                    158.0,
                    200.0,
                    tr("remove", "Remove from This Device"),
                    true,
                    REMOVE,
                );
                let asked = shared_worlds::details(id).open_request().is_some();
                push(
                    cx - 100.0,
                    182.0,
                    200.0,
                    if asked {
                        tr("backups_asked", "Backups (a load is asked)...")
                    } else {
                        tr("backups", "Backups...")
                    },
                    true,
                    BACKUPS,
                );
                push(
                    cx - 100.0,
                    h - 27.0,
                    200.0,
                    tr("back", "Back"),
                    true,
                    WORLD_BACK,
                );
            }
            Page::Remove(_) => {
                push(
                    cx - 154.0,
                    h - 27.0,
                    150.0,
                    tr("remove_yes", "Remove"),
                    true,
                    REMOVE_CONFIRM,
                );
                push(
                    cx + 4.0,
                    h - 27.0,
                    150.0,
                    tr("cancel", "Cancel"),
                    true,
                    REMOVE_CANCEL,
                );
            }
            Page::Add => {
                push(cx - 100.0, 86.0, 200.0, tr("paste", "Paste"), true, PASTE);
                let ready = !self.invite.trim().is_empty();
                push(
                    cx - 154.0,
                    h - 27.0,
                    150.0,
                    tr("add_world", "Add World"),
                    ready,
                    ADD_CONFIRM,
                );
                push(
                    cx + 4.0,
                    h - 27.0,
                    150.0,
                    tr("cancel", "Cancel"),
                    true,
                    ADD_CANCEL,
                );
            }
            Page::Pick => {
                let n = rows(h);
                let offset = self.offset.min(self.local.len().saturating_sub(1) / n * n);
                for (i, (_, name)) in self.local.iter().skip(offset).take(n).enumerate() {
                    push(
                        cx - 150.0,
                        56.0 + 24.0 * i as f32,
                        300.0,
                        name.clone(),
                        true,
                        PICK_ROW + i as u8,
                    );
                }
                if self.local.len() > n {
                    push(
                        cx - 176.0,
                        56.0,
                        20.0,
                        "<".into(),
                        offset > 0,
                        PICK_PREVIOUS,
                    );
                    push(
                        cx + 156.0,
                        56.0,
                        20.0,
                        ">".into(),
                        offset + n < self.local.len(),
                        PICK_NEXT,
                    );
                }
                push(
                    cx - 100.0,
                    h - 27.0,
                    200.0,
                    tr("cancel", "Cancel"),
                    true,
                    PICK_CANCEL,
                );
            }
            Page::Help { .. } => {
                push(
                    cx - 100.0,
                    h - 52.0,
                    200.0,
                    tr("open_cloudflare", "Open Cloudflare"),
                    true,
                    HELP_OPEN,
                );
                push(
                    cx - 100.0,
                    h - 27.0,
                    200.0,
                    tr("back", "Back"),
                    true,
                    HELP_BACK,
                );
            }
            Page::Backups(id) => {
                let details = shared_worlds::details(id);
                // The world as it is now is the first: the others can be loaded.
                let older: Vec<&backups::Backup> = details.backups.iter().skip(1).collect();
                let n = backup_rows(h);
                let offset = self.offset.min(older.len().saturating_sub(1) / n * n);
                let open = details.open_request().is_some();
                for (i, backup) in older.iter().skip(offset).take(n).enumerate() {
                    push(
                        cx - 150.0,
                        BACKUP_TOP + 24.0 * i as f32,
                        300.0,
                        backup_label(backup),
                        !open,
                        BACKUP_ROW + i as u8,
                    );
                }
                if older.len() > n {
                    push(
                        cx - 176.0,
                        BACKUP_TOP,
                        20.0,
                        "<".into(),
                        offset > 0,
                        BACKUP_PREVIOUS,
                    );
                    push(
                        cx + 156.0,
                        BACKUP_TOP,
                        20.0,
                        ">".into(),
                        offset + n < older.len(),
                        BACKUP_NEXT,
                    );
                }
                // What can be done about loads now, two at most.
                let mut actions: Vec<(String, u8)> = Vec::new();
                let me = details.me.as_ref();
                match details.open_request() {
                    Some(r) if r.state == backups::State::Asked && details.owner_is_me() => {
                        actions.push((tr("approve", "Approve Load"), APPROVE));
                        actions.push((tr("decline", "Decline"), DECLINE));
                    }
                    Some(r) if details.owner_is_me() || me.is_some_and(|m| m.uuid == r.by.uuid) => {
                        actions.push((tr("withdraw", "Withdraw Request"), WITHDRAW));
                    }
                    Some(_) => {}
                    None => {
                        if details
                            .loaded
                            .as_ref()
                            .is_some_and(|l| l.replaced.is_some())
                        {
                            actions.push((tr("undo", "Undo Last Load"), UNDO));
                        }
                    }
                }
                if details.owner == Some(Owner::Unknown) {
                    actions.push((tr("claim", "I Shared It"), CLAIM));
                }
                match actions.as_slice() {
                    [(label, id)] => push(cx - 100.0, h - 52.0, 200.0, label.clone(), true, *id),
                    [a, b, ..] => {
                        push(cx - 154.0, h - 52.0, 150.0, a.0.clone(), true, a.1);
                        push(cx + 4.0, h - 52.0, 150.0, b.0.clone(), true, b.1);
                    }
                    [] => {}
                }
                // How often a backup is kept: the owner's to choose, shown to everyone.
                push(
                    cx - 154.0,
                    h - 27.0,
                    150.0,
                    if details.read {
                        tr("every_button", "Backups: every %d min")
                            .replacen("%d", &details.every.to_string(), 1)
                    } else {
                        tr("every_unread", "Backups: ...")
                    },
                    details.read && details.owner_is_me(),
                    BACKUP_EVERY,
                );
                push(
                    cx + 4.0,
                    h - 27.0,
                    150.0,
                    tr("back", "Back"),
                    true,
                    BACKUP_BACK,
                );
            }
            Page::Load { world, .. } => {
                let mine = shared_worlds::details(world).owner_is_me();
                push(
                    cx - 154.0,
                    h - 27.0,
                    150.0,
                    if mine {
                        tr("load", "Load")
                    } else {
                        tr("ask_load", "Ask to Load")
                    },
                    true,
                    LOAD_CONFIRM,
                );
                push(
                    cx + 4.0,
                    h - 27.0,
                    150.0,
                    tr("cancel", "Cancel"),
                    true,
                    LOAD_CANCEL,
                );
            }
            Page::Storage { .. } => {
                push(
                    cx - 154.0,
                    h - 52.0,
                    150.0,
                    tr("paste", "Paste"),
                    true,
                    PASTE,
                );
                push(
                    cx + 4.0,
                    h - 52.0,
                    150.0,
                    tr("setup_help", "How to Set Up..."),
                    true,
                    STORAGE_HELP,
                );
                let ready = self.storage_ready();
                push(
                    cx - 154.0,
                    h - 27.0,
                    150.0,
                    tr("share_world", "Share World"),
                    ready,
                    STORAGE_SHARE,
                );
                push(
                    cx + 4.0,
                    h - 27.0,
                    150.0,
                    tr("cancel", "Cancel"),
                    true,
                    STORAGE_CANCEL,
                );
            }
        }
        out
    }

    /// The world list's rows on the current page, for a row button's id.
    fn row(&self, id: u8, first: u8, h: f32, length: usize) -> Option<usize> {
        let n = rows(h);
        let offset = self.offset.min(length.saturating_sub(1) / n * n);
        let index = offset + usize::from(id - first);
        (index < length).then_some(index)
    }

    pub(super) fn click(&mut self, id: u8, h: f32, saves: &std::path::Path) -> Outcome {
        match id {
            TASK_CANCEL => shared_worlds::cancel(),
            TASK_OK => {
                if matches!(shared_worlds::task(), Task::Done(_)) {
                    self.page = Page::List;
                    self.offset = 0;
                    for field in &mut self.storage {
                        field.clear();
                    }
                }
                shared_worlds::dismiss();
            }
            BACK => return Outcome::Close,
            PREVIOUS | PICK_PREVIOUS => self.offset = self.offset.saturating_sub(rows(h)),
            NEXT | PICK_NEXT => self.offset += rows(h),
            ROW..=215 => {
                let entries = shared_worlds::entries();
                if let Some(i) = self.row(id, ROW, h, entries.len()) {
                    self.page = Page::World(entries[i].id.clone());
                    self.notice = None;
                    shared_worlds::refresh(true);
                    shared_worlds::read_details(&entries[i].id);
                }
            }
            PLAY => {
                if let Page::World(world) = &self.page {
                    return Outcome::Play(world.clone());
                }
            }
            COPY => {
                if let Page::World(world) = &self.page
                    && let Some(text) = shared_worlds::invite_text(world)
                {
                    SystemClipboard::default().set(&text);
                    self.copied = Some(Instant::now());
                    self.notice = Some(tr(
                        "invite_secret",
                        "Send it only to friends: whoever has it can play and change this world.",
                    ));
                }
            }
            REMOVE => {
                if let Page::World(world) = &self.page {
                    self.page = Page::Remove(world.clone());
                }
            }
            REMOVE_CONFIRM => {
                if let Page::Remove(world) = &self.page {
                    let world = world.clone();
                    self.page = Page::List;
                    self.notice = shared_worlds::remove(&world).err();
                }
            }
            REMOVE_CANCEL | WORLD_BACK | ADD_CANCEL | PICK_CANCEL | STORAGE_CANCEL => {
                self.leave_page()
            }
            ADD => {
                self.page = Page::Add;
                self.invite.clear();
                self.notice = None;
            }
            PASTE => {
                let text = SystemClipboard::default().get();
                let text = text.trim().to_owned();
                if self.storage_page() && self.focus == 0 {
                    let (address, bucket) = storage_address(&text);
                    self.storage[0] = address.chars().take(STORAGE_FIELDS[0].2).collect();
                    if let Some(bucket) = bucket
                        && self.storage[1].trim().is_empty()
                    {
                        self.storage[1] = bucket.chars().take(STORAGE_FIELDS[1].2).collect();
                    }
                } else if let Some((_, field, max)) = self.field() {
                    *field = text.chars().take(max).collect();
                }
            }
            ADD_CONFIRM => match shared_worlds::add(&self.invite) {
                Ok(name) => {
                    self.invite.clear();
                    self.page = Page::List;
                    self.notice = Some(tr("added", "Added %s.").replacen("%s", &name, 1));
                }
                Err(e) => self.notice = Some(e),
            },
            SHARE => {
                self.local = crate::title::list_worlds(saves)
                    .into_iter()
                    .filter(|w| !w.corrupted && !w.locked && w.incompatible.is_none())
                    .map(|w| (w.folder, w.name))
                    .collect();
                self.offset = 0;
                self.notice = None;
                self.page = Page::Pick;
            }
            PICK_ROW..=235 => {
                if let Some(i) = self.row(id, PICK_ROW, h, self.local.len()) {
                    let (folder, name) = self.local[i].clone();
                    if let Some(bucket) = shared_worlds::last_bucket() {
                        self.storage = [
                            bucket.endpoint,
                            bucket.bucket,
                            bucket.access_key,
                            bucket.secret_key,
                        ];
                    }
                    self.focus = self.storage.iter().position(|f| f.is_empty()).unwrap_or(0);
                    self.page = Page::Storage { folder, name };
                }
            }
            STORAGE_HELP => {
                if let Page::Storage { folder, name } = &self.page {
                    self.page = Page::Help {
                        folder: folder.clone(),
                        name: name.clone(),
                    };
                }
            }
            HELP_OPEN => {
                let _ = crate::platform::open_link(CLOUDFLARE_URL);
            }
            HELP_BACK | BACKUP_BACK | LOAD_CANCEL => self.leave_page(),
            BACKUP_EVERY => {
                if let Page::Backups(world) = &self.page {
                    let details = shared_worlds::details(world);
                    if details.read && details.owner_is_me() {
                        shared_worlds::set_backup_every(world, next_every(details.every));
                    }
                }
            }
            BACKUPS => {
                if let Page::World(world) = &self.page {
                    shared_worlds::read_details(world);
                    self.page = Page::Backups(world.clone());
                    self.offset = 0;
                    self.notice = None;
                }
            }
            BACKUP_PREVIOUS => self.offset = self.offset.saturating_sub(backup_rows(h)),
            BACKUP_NEXT => self.offset += backup_rows(h),
            BACKUP_ROW..=228 => {
                if let Page::Backups(world) = &self.page {
                    let details = shared_worlds::details(world);
                    let older = details.backups.len().saturating_sub(1);
                    let n = backup_rows(h);
                    let offset = self.offset.min(older.saturating_sub(1) / n * n);
                    let index = offset + usize::from(id - BACKUP_ROW);
                    if let Some(backup) = details.backups.get(index + 1)
                        && details.open_request().is_none()
                    {
                        self.page = Page::Load {
                            world: world.clone(),
                            backup: backup.id,
                            undo: false,
                        };
                    }
                }
            }
            UNDO => {
                if let Page::Backups(world) = &self.page
                    && let Some(replaced) = shared_worlds::details(world)
                        .loaded
                        .and_then(|l| l.replaced)
                {
                    self.page = Page::Load {
                        world: world.clone(),
                        backup: replaced,
                        undo: true,
                    };
                }
            }
            LOAD_CONFIRM => {
                if let Page::Load {
                    world,
                    backup,
                    undo,
                } = &self.page
                {
                    shared_worlds::ask_load(world, *backup, *undo);
                    self.page = Page::Backups(world.clone());
                }
            }
            APPROVE | DECLINE => {
                if let Page::Backups(world) = &self.page {
                    shared_worlds::decide(world, id == APPROVE);
                }
            }
            WITHDRAW => {
                if let Page::Backups(world) = &self.page {
                    shared_worlds::withdraw(world);
                }
            }
            CLAIM => {
                if let Page::Backups(world) = &self.page {
                    shared_worlds::claim_owner(world);
                }
            }
            STORAGE_SHARE => {
                if let Page::Storage { folder, name } = &self.page
                    && self.storage_ready()
                {
                    shared_worlds::create(folder.clone(), name.clone(), self.bucket());
                }
            }
            _ => {}
        }
        Outcome::None
    }
}

/// A task's progress or result covers the page.
fn task_shown() -> bool {
    !matches!(shared_worlds::task(), Task::Idle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setup_help_fits_above_its_buttons_on_a_phone() {
        let atlas =
            crate::hud::HudAtlas::build(std::path::Path::new("/nonexistent/horizon-test-assets"))
                .unwrap();
        for (w, h) in [(427.0f32, 240.0f32), (341.0, 256.0), (520.0, 240.0)] {
            let width = (w - 50.0).min(320.0);
            let mut y = 34.0;
            for paragraph in help_text().split('\n') {
                if paragraph.is_empty() {
                    y += 4.0;
                } else {
                    y += 10.0 * super::wrapped(&atlas, paragraph, width).len() as f32;
                }
            }
            assert!(y <= h - 52.0, "{w}x{h}: the text ends at {y}");
        }
        // Four steps, each saying where to tap.
        for step in ["1. ", "2. ", "3. ", "4. "] {
            assert!(help_text().contains(step));
        }
    }

    #[test]
    fn the_backups_page_keeps_its_rows_above_the_storage_line_and_the_buttons() {
        for h in [240.0f32, 256.0, 360.0] {
            let n = backup_rows(h);
            assert!(n >= 2, "{h}: {n}");
            let bottom = BACKUP_TOP + 24.0 * (n as f32 - 1.0) + 20.0;
            assert!(bottom <= h - 64.0 - 2.0, "{h}: the rows end at {bottom}");
        }
        // Six rows at most, inside their ids.
        assert!(BACKUP_ROW + 6 <= BACKUP_PREVIOUS);
        assert!(
            usage_line(&shared_worlds::Details {
                usage: Some((1_234_000_000, 250_000_000)),
                ..Default::default()
            })
            .is_some_and(|l| l.chars().count() <= 50)
        );
    }

    #[test]
    fn a_pasted_storage_address_is_taken_as_cloudflare_shows_it() {
        let account = "0123456789abcdef0123456789ABCDEF";
        assert_eq!(
            storage_address(account),
            (
                "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com".into(),
                None
            )
        );
        assert_eq!(
            storage_address(" https://abc.r2.cloudflarestorage.com/horizon-worlds/ "),
            (
                "https://abc.r2.cloudflarestorage.com".into(),
                Some("horizon-worlds".into())
            )
        );
        assert_eq!(
            storage_address("https://abc.r2.cloudflarestorage.com"),
            ("https://abc.r2.cloudflarestorage.com".into(), None)
        );
        assert_eq!(
            storage_address("https://abc.r2.cloudflarestorage.com/a/b"),
            ("https://abc.r2.cloudflarestorage.com".into(), None)
        );
        assert_eq!(storage_address("not a url"), ("not a url".into(), None));
    }

    #[test]
    fn the_backup_pace_cycles_through_the_owners_choices() {
        let mut seen = vec![backups::EVERY_DEFAULT];
        for _ in 0..backups::EVERY.len() {
            seen.push(next_every(*seen.last().unwrap()));
        }
        // From the default round every choice and back to it; an unknown one starts over.
        assert_eq!(seen, [5, 10, 15, 30, 60, 2, 5]);
        assert_eq!(next_every(7), backups::EVERY[0]);
        assert!(BACKUP_EVERY < ADD_CONFIRM && !(ROW..ROW + 6).contains(&BACKUP_EVERY));
    }

    #[test]
    fn rows_fit_the_screen() {
        assert_eq!(rows(262.0), 5);
        assert_eq!(rows(140.0), 1);
        assert_eq!(rows(2000.0), 6);
        // Six rows at most: their ids stay below the next range.
        assert!(ROW + 6 <= ADD_CONFIRM && PICK_ROW + 6 <= PICK_PREVIOUS);
    }

    #[test]
    fn a_status_reads_as_who_is_there() {
        assert_eq!(
            status_line(&Status::Known {
                playing: 0,
                host: None,
                open: false,
                tunnel: false,
                silent: false,
                away: false,
            }),
            "Nobody playing"
        );
        assert_eq!(
            status_line(&Status::Known {
                playing: 3,
                host: Some("Alex".into()),
                open: true,
                tunnel: false,
                silent: false,
                away: false,
            }),
            "3 playing, hosted by Alex"
        );
        assert_eq!(
            status_line(&Status::Known {
                playing: 1,
                host: Some("Alex".into()),
                open: false,
                tunnel: false,
                silent: false,
                away: false,
            }),
            "Alex is opening it"
        );
        // A host that stopped answering isn't shown as playing, open or opening.
        for open in [true, false] {
            assert_eq!(
                status_line(&Status::Known {
                    playing: 1,
                    host: Some("Alex".into()),
                    open,
                    tunnel: false,
                    silent: true,
                    away: false,
                }),
                "Alex's game isn't answering"
            );
        }
        assert_eq!(
            status_line(&Status::Known {
                playing: 1,
                host: Some("Alex".into()),
                open: true,
                tunnel: false,
                silent: false,
                away: true,
            }),
            "Alex's game is in the background"
        );
    }

    #[test]
    fn the_secret_key_is_masked_and_the_fields_cycle() {
        let mut screen = Screen {
            page: Page::Storage {
                folder: "f".into(),
                name: "n".into(),
            },
            ..Screen::default()
        };
        assert!(!screen.password_masked());
        for _ in 0..3 {
            assert!(screen.tab(false));
        }
        assert!(screen.password_masked());
        assert!(screen.tab(false));
        assert_eq!(screen.focus, 0);
        assert!(screen.tab(true));
        assert_eq!(screen.focus, 3);
        assert!(!format!("{screen:?}").contains("secret"));
    }
}
