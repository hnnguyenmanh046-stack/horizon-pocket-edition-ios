//! Voice chat in a shared world, in the window's game: whether it is on and whether the
//! microphone is, what the host is told, who is in reach, the lines in chat that say it is
//! there, and what the player chose for each of the others (`horizon-voice` codes, buffers
//! and mixes; `lan::voice` relays on the host; `voice_net` is this game's end of the
//! connection).
//!
//! It is off when a world is joined, every time, and so is the microphone when it is turned
//! on: nothing is heard, and nothing is said, that the player didn't ask for. Both stay as
//! they are while the world changes hosts (the connection is made again, the session isn't).
use super::Game;
use horizon_client::{
    Result, lang,
    text::Text,
    voice_net::{Link, Sender, Told},
};
use horizon_voice::{
    engine::{CaptureOptions, CapturePipe, Playout},
    settings::Settings,
    wire::{Entry, Up, VERSION},
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

fn tr(key: &str, english: &str) -> String {
    lang::get_or(&format!("horizon.voice.{key}"), english)
}

/// The colour of voice chat's lines in chat: gold, so they stand out from the rest.
const HINT: u32 = 0xFF_AA00;
/// The file the choices are kept in, in the player's folder.
const FILE: &str = "voice-chat.json";
/// A connection lost with the microphone live (the world changes hosts): live again when it
/// is back within this, muted otherwise.
const MIC_KEPT_FOR: Duration = Duration::from_secs(30);

/// The others' voices as the sound output asks for them.
struct Tap(Arc<Playout>);

impl horizon_client::audio::StreamSource for Tap {
    fn render(&self, out: &mut [f32], channels: usize, rate: u32) -> bool {
        self.0.render(out, channels, rate)
    }
}

/// The end-to-end checks' ears (`HORIZON_VOICE_TEST=1`, a game with no sound output): pulls
/// the voices as an output would and keeps what it heard.
struct Probe {
    heard: Arc<Mutex<Heard>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default, Clone, Copy)]
struct Heard {
    /// The level and pitch of the last 200 ms that had sound.
    level: f32,
    hz: f32,
    /// Blocks of 20 ms with sound, ever, and since when the last stretch of them runs.
    blocks: u64,
    quiet_blocks: u64,
}

impl Probe {
    fn start(playout: Arc<Playout>) -> Self {
        let heard: Arc<Mutex<Heard>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (heard, stop) = (heard.clone(), stop.clone());
            std::thread::Builder::new()
                .name("horizon-voice-probe".into())
                .spawn(move || {
                    let mut window: Vec<f32> = Vec::new();
                    let started = Instant::now();
                    let mut blocks = 0u32;
                    while !stop.load(Ordering::SeqCst) {
                        // An output's pace: 20 ms of sound every 20 ms.
                        let due = started + Duration::from_millis(20) * blocks;
                        if let Some(wait) = due.checked_duration_since(Instant::now()) {
                            std::thread::sleep(wait);
                        }
                        blocks += 1;
                        let mut block = [0.0f32; 960];
                        playout.render(&mut block, 1, 48_000);
                        let level = horizon_voice::dsp::rms(&block);
                        let mut heard = heard.lock().unwrap_or_else(|e| e.into_inner());
                        if level < 0.005 {
                            heard.quiet_blocks += 1;
                            window.clear();
                            continue;
                        }
                        heard.blocks += 1;
                        heard.quiet_blocks = 0;
                        window.extend_from_slice(&block);
                        if window.len() >= 9_600 {
                            // The pitch of a tone: its rising zero crossings a second.
                            let crossings = window
                                .windows(2)
                                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                                .count();
                            heard.hz = crossings as f32 * 48_000.0 / window.len() as f32;
                            heard.level = horizon_voice::dsp::rms(&window);
                            window.clear();
                        }
                    }
                })
                .ok()
        };
        Self {
            heard,
            stop,
            thread,
        }
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The end-to-end checks' mouth: a tone fed to the microphone's pipe at a microphone's pace.
struct Tone {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Tone {
    fn start(pipe: Arc<CapturePipe>, hz: f32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("horizon-voice-tone".into())
                .spawn(move || {
                    let started = Instant::now();
                    let mut blocks = 0u32;
                    let mut n = 0u64;
                    while !stop.load(Ordering::SeqCst) {
                        let due = started + Duration::from_millis(10) * blocks;
                        if let Some(wait) = due.checked_duration_since(Instant::now()) {
                            std::thread::sleep(wait);
                        }
                        blocks += 1;
                        let mut block = [0.0f32; 480];
                        for s in &mut block {
                            let t = n as f32 / 48_000.0;
                            *s = 0.2 * (std::f32::consts::TAU * hz * t).sin();
                            n += 1;
                        }
                        pipe.feed(&block, 1, 48_000);
                    }
                })
                .ok()
        };
        Self { stop, thread }
    }
}

impl Drop for Tone {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The microphone while it is live: its pipe to the connection, and what feeds it.
struct Mic {
    pipe: Arc<CapturePipe>,
    _tone: Option<Tone>,
}

/// One player in voice chat's reach, for the screens.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Person {
    pub uuid: u128,
    pub name: String,
    /// Voice chat is on in their game, and their microphone is.
    pub enabled: bool,
    pub mic_live: bool,
    /// How this player hears them: 0 to 2, and not at all.
    pub volume: f32,
    pub muted: bool,
    pub talking: bool,
    pub level: f32,
    pub delay_ms: u32,
}

pub(super) struct Voice {
    settings: Settings,
    /// The choices changed and aren't written yet, since when.
    unsaved: Option<Instant>,
    playout: Arc<Playout>,
    /// Voice chat is on: the others are heard.
    enabled: bool,
    /// The microphone is meant to be live.
    mic_wanted: bool,
    mic: Option<Mic>,
    /// The connection voice chat runs on now, and its way out.
    link: Option<Arc<Link>>,
    sender: Option<Sender>,
    /// This game said it has voice chat on that connection; the number the host gave it.
    joined: bool,
    id: Option<u8>,
    roster: Vec<Entry>,
    /// What the host was last told.
    told_state: Option<(bool, bool)>,
    told_unwanted: Option<Vec<u8>>,
    /// The lines already shown this session: that it is there, and who uses it.
    hinted: bool,
    hinted_users: HashSet<u128>,
    /// Every line shown, for the end-to-end checks.
    lines: Vec<String>,
    /// When the connection went with the microphone live.
    mic_lost: Option<Instant>,
    probe: Option<Probe>,
    /// The checks' tone, when one was asked for.
    tone: Option<f32>,
}

fn file() -> PathBuf {
    horizon_client::logging::player_dir().join(FILE)
}

fn testing() -> bool {
    std::env::var_os("HORIZON_VOICE_TEST").is_some_and(|v| v == "1")
}

impl Voice {
    pub(super) fn new() -> Self {
        let settings = std::fs::read_to_string(file())
            .map(|text| Settings::parse(&text))
            .unwrap_or_default();
        let playout = Arc::new(Playout::default());
        playout.set_master(settings.voices);
        Self {
            settings,
            unsaved: None,
            playout,
            enabled: false,
            mic_wanted: false,
            mic: None,
            link: None,
            sender: None,
            joined: false,
            id: None,
            roster: Vec::new(),
            told_state: None,
            told_unwanted: None,
            hinted: false,
            hinted_users: HashSet::new(),
            lines: Vec::new(),
            mic_lost: None,
            probe: None,
            tone: None,
        }
    }

    /// Voice chat is on (the others are heard).
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    /// The microphone is live.
    pub(super) fn mic_live(&self) -> bool {
        self.mic.is_some()
    }

    /// The host carries voices and this game is in their reach.
    pub(super) fn connected(&self) -> bool {
        self.id.is_some()
    }

    /// How loud the microphone hears now, and whether speech is going out.
    pub(super) fn mic_level(&self) -> (f32, bool) {
        self.mic
            .as_ref()
            .map_or((0.0, false), |m| (m.pipe.level(), m.pipe.speaking()))
    }

    pub(super) fn voices_volume(&self) -> f32 {
        self.settings.voices
    }

    /// The microphone chosen ("" for the automatic choice).
    pub(super) fn mic_choice(&self) -> &str {
        &self.settings.mic
    }

    /// The others in reach, as this player hears them.
    pub(super) fn people(&self) -> Vec<Person> {
        let heard = self.playout.heard();
        self.roster
            .iter()
            .filter(|e| Some(e.id) != self.id)
            .map(|e| {
                let uuid = format!("{:032x}", e.uuid);
                let now = heard.iter().find(|h| h.id == e.id);
                Person {
                    uuid: e.uuid,
                    name: e.name.clone(),
                    enabled: e.enabled,
                    mic_live: e.mic_live,
                    volume: self.settings.volume_of(&uuid),
                    muted: self.settings.is_muted(&uuid),
                    talking: now.is_some_and(|h| h.talking),
                    level: now.map_or(0.0, |h| h.level),
                    delay_ms: now.map_or(0, |h| h.delay_ms),
                }
            })
            .collect()
    }

    /// Everyone's settings as the engine plays them, and the speakers the host needn't send.
    fn apply(&mut self) {
        let mut unwanted = Vec::new();
        let mut ids = Vec::new();
        for e in &self.roster {
            if Some(e.id) == self.id {
                continue;
            }
            let uuid = format!("{:032x}", e.uuid);
            let (volume, muted) = (self.settings.volume_of(&uuid), self.settings.is_muted(&uuid));
            self.playout.set_speaker(e.id, volume, muted);
            ids.push(e.id);
            if muted || volume == 0.0 {
                unwanted.push(e.id);
            }
        }
        self.playout.keep(&ids);
        self.playout.set_master(self.settings.voices);
        if self.told_unwanted.as_ref() != Some(&unwanted)
            && self.id.is_some()
            && let Some(sender) = &self.sender
        {
            sender.send(
                Up::Listen {
                    muted: unwanted.clone(),
                }
                .encode(),
            );
            self.told_unwanted = Some(unwanted);
        }
    }

    fn changed(&mut self) {
        self.unsaved.get_or_insert_with(Instant::now);
        self.apply();
    }

    pub(super) fn set_volume(&mut self, uuid: u128, volume: f32) {
        if self.settings.set_volume(&format!("{uuid:032x}"), volume) {
            self.changed();
        }
    }

    pub(super) fn set_muted(&mut self, uuid: u128, muted: bool) {
        if self.settings.set_muted(&format!("{uuid:032x}"), muted) {
            self.changed();
        }
    }

    pub(super) fn set_voices_volume(&mut self, volume: f32) {
        let volume = volume.clamp(0.0, horizon_voice::engine::MAX_VOLUME);
        if self.settings.voices != volume {
            self.settings.voices = volume;
            self.changed();
        }
    }

    pub(super) fn set_mic_choice(&mut self, name: &str) {
        if self.settings.mic != name {
            self.settings.mic = name.to_owned();
            self.unsaved.get_or_insert_with(Instant::now);
            // The microphone in use changes with the choice.
            if self.mic.is_some() {
                self.mic = None;
            }
        }
    }

    /// Writes the choices once they have stood a moment (or at once, with `now`).
    fn save(&mut self, now: bool) {
        let due = self
            .unsaved
            .is_some_and(|since| now || since.elapsed() >= Duration::from_secs(1));
        if !due {
            return;
        }
        self.unsaved = None;
        if let Err(e) = horizon_shared_worlds::scan::write_atomic(
            &file(),
            self.settings.to_json().as_bytes(),
        ) {
            log::warn!("Voice chat: its settings weren't written: {e}");
        }
    }

    /// The sound output hears the voices, or doesn't.
    fn listen(&mut self, on: bool) {
        if let Some(link) = &self.link {
            link.attach(on.then(|| self.playout.clone()));
        }
        if testing() {
            // No sound output in the checks: their own ears, at an output's pace.
            self.probe = on.then(|| Probe::start(self.playout.clone()));
        } else {
            let tap: Option<Arc<dyn horizon_client::audio::StreamSource>> = if on {
                Some(Arc::new(Tap(self.playout.clone())))
            } else {
                None
            };
            horizon_client::audio::set_stream_source(tap);
        }
        if !on {
            self.playout.clear();
        }
    }

    /// Forgets the connection (it ended, or the session did).
    fn disconnected(&mut self) {
        if self.mic.take().is_some() {
            self.mic_lost = Some(Instant::now());
        }
        if let Some(link) = self.link.take() {
            link.attach(None);
        }
        self.sender = None;
        self.joined = false;
        self.id = None;
        self.roster.clear();
        self.told_state = None;
        self.told_unwanted = None;
        self.playout.clear();
    }

    /// The session ended: everything is as before a world is joined.
    fn reset(&mut self) {
        self.disconnected();
        if self.enabled {
            self.listen(false);
        }
        self.enabled = false;
        self.mic_wanted = false;
        self.mic_lost = None;
        self.hinted = false;
        self.hinted_users.clear();
        self.tone = None;
        self.save(true);
    }
}

impl Game<'_> {
    /// A line of voice chat's in chat: coloured, so it isn't missed.
    fn voice_line(&mut self, text: String) {
        self.voice.lines.push(text.clone());
        if self.voice.lines.len() > 50 {
            self.voice.lines.remove(0);
        }
        self.system_message(Text::literal(text).with_color(HINT).bold());
    }

    /// Voice chat can be turned on here: a shared world whose host carries voices.
    pub(super) fn voice_offered(&self) -> bool {
        self.shared_active()
            && self
                .remote
                .as_ref()
                .is_some_and(|r| r.session.voice.offered())
    }

    /// Turns voice chat on (the others are heard; the microphone stays off) or off.
    pub(super) fn voice_set_enabled(&mut self, on: bool) {
        if self.voice.enabled == on || (on && !self.voice_offered()) {
            return;
        }
        self.voice.enabled = on;
        self.voice.listen(on);
        if on {
            self.voice.apply();
            self.voice_line(tr(
                "on",
                "Voice chat is on: you hear the others. Your microphone is off until you turn it on.",
            ));
        } else {
            self.voice.mic_wanted = false;
            self.voice.mic = None;
            self.voice.mic_lost = None;
            self.voice_line(tr("off", "Voice chat is off."));
        }
    }

    /// Turns the microphone on or off (only with voice chat on).
    pub(super) fn voice_set_mic(&mut self, live: bool) {
        if !self.voice.enabled {
            return;
        }
        self.voice.mic_wanted = live;
        self.voice.mic_lost = None;
        if !live {
            self.voice.mic = None;
        }
    }

    pub(super) fn voice_toggle_mic(&mut self) {
        let live = !self.voice.mic_wanted;
        self.voice_set_mic(live);
    }

    /// Starts the microphone: its pipe to the connection, and the device (or the checks'
    /// tone) that feeds it.
    fn voice_start_mic(&mut self) {
        let Some(sender) = self.voice.sender.clone() else {
            return;
        };
        let pipe = match CapturePipe::start(
            Box::new(move |message| sender.send(message)),
            CaptureOptions::default(),
        ) {
            Ok(pipe) => Arc::new(pipe),
            Err(e) => {
                self.voice.mic_wanted = false;
                self.voice_line(
                    tr("mic_failed", "The microphone couldn't be started: %s")
                        .replacen("%s", &e.to_string(), 1),
                );
                return;
            }
        };
        let tone = self.voice.tone.map(|hz| Tone::start(pipe.clone(), hz));
        self.voice.mic = Some(Mic { pipe, _tone: tone });
    }

    /// Voice chat's work for this frame.
    pub(super) fn tick_voice(&mut self) {
        // Only in a shared world, and only while its session lasts.
        if !self.shared_active() {
            if self.voice.enabled || self.voice.link.is_some() {
                self.voice.reset();
            }
            self.voice.save(false);
            return;
        }
        // The connection of the moment: another one after a host change.
        let link = self
            .remote
            .as_ref()
            .filter(|r| r.joined)
            .map(|r| r.session.voice.clone());
        let same = match (&self.voice.link, &link) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.voice.disconnected();
            if let (Some(link), Some(remote)) = (link, self.remote.as_ref()) {
                self.voice.sender = Some(remote.session.voice_sender());
                if self.voice.enabled {
                    link.attach(Some(self.voice.playout.clone()));
                }
                self.voice.link = Some(link);
            }
        }
        let Some(link) = self.voice.link.clone() else {
            // Between two hosts: a microphone that was live waits a while for the next.
            if self
                .voice
                .mic_lost
                .is_some_and(|at| at.elapsed() > MIC_KEPT_FOR)
                && self.voice.mic_wanted
            {
                self.voice.mic_wanted = false;
                self.voice.mic_lost = None;
                self.voice_line(tr(
                    "mic_muted_away",
                    "Your microphone was turned off while the world changed hosts.",
                ));
            }
            return;
        };
        // The host carries voices: this game says it does too, once.
        if link.offered()
            && !self.voice.joined
            && let Some(sender) = &self.voice.sender
        {
            sender.send(Up::Join { version: VERSION }.encode());
            self.voice.joined = true;
        }
        for told in link.take() {
            match told {
                Told::Welcome { id } => {
                    self.voice.id = Some(id);
                    self.voice.told_state = None;
                    self.voice.told_unwanted = None;
                }
                Told::Roster(entries) => {
                    self.voice.roster = entries;
                    self.voice.apply();
                    self.voice_hints();
                }
            }
        }
        if self.voice.id.is_none() {
            return;
        }
        // The microphone, as it is meant to be.
        if self.voice.enabled && self.voice.mic_wanted && self.voice.mic.is_none() {
            self.voice.mic_lost = None;
            self.voice_start_mic();
        }
        let state = (self.voice.enabled, self.voice.mic.is_some());
        if self.voice.told_state != Some(state)
            && let Some(sender) = &self.voice.sender
        {
            sender.send(
                Up::State {
                    enabled: state.0,
                    mic_live: state.1,
                }
                .encode(),
            );
            self.voice.told_state = Some(state);
        }
        self.voice.save(false);
    }

    /// The lines that say voice chat is there, to a player who has it off: once that it is,
    /// and once for each of the others who uses it.
    fn voice_hints(&mut self) {
        if self.voice.enabled {
            return;
        }
        let me = self.voice.id;
        let others: Vec<Entry> = self
            .voice
            .roster
            .iter()
            .filter(|e| Some(e.id) != me)
            .cloned()
            .collect();
        let mut said_how = false;
        for other in others.iter().filter(|e| e.enabled) {
            if self.voice.hinted_users.insert(other.uuid) {
                self.voice_line(
                    tr(
                        "in_use",
                        "%s is using voice chat. Turn it on in Game Menu > Friends > Voice Chat to hear them.",
                    )
                    .replacen("%s", &other.name, 1),
                );
                said_how = true;
            }
        }
        if !others.is_empty() && !self.voice.hinted {
            self.voice.hinted = true;
            if !said_how {
                self.voice_line(tr(
                    "available",
                    "Voice chat is available in this world. Turn it on in Game Menu > Friends > Voice Chat.",
                ));
            }
        }
    }

    /// The window is closing: what was chosen is written.
    pub(super) fn voice_save_now(&mut self) {
        self.voice.save(true);
    }

    /// `horizon-ctl voice ...`, for the end-to-end checks.
    pub(super) fn voice_control(&mut self, words: &[&str]) -> Result<Option<serde_json::Value>> {
        use serde_json::json;
        if words.first() != Some(&"voice") {
            return Ok(None);
        }
        match (words.get(1).copied(), words.get(2).copied()) {
            (Some("enable"), _) => self.voice_set_enabled(true),
            (Some("disable"), _) => self.voice_set_enabled(false),
            (Some("mic"), Some("on")) => self.voice_set_mic(true),
            (Some("mic"), Some("off")) => self.voice_set_mic(false),
            (Some("mic"), Some("toggle")) => self.voice_toggle_mic(),
            (Some("input"), name) => self.voice.set_mic_choice(name.unwrap_or("")),
            (Some("voices"), Some(volume)) => {
                let volume = volume.parse().map_err(|_| "voice voices 0..2")?;
                self.voice.set_voices_volume(volume);
            }
            // The checks' microphone: a tone of this pitch, from the next time it is live.
            (Some("tone"), Some(hz)) => {
                self.voice.tone = Some(hz.parse().map_err(|_| "voice tone HZ")?);
                self.voice.mic = None;
            }
            (Some("mute"), Some(name)) | (Some("unmute"), Some(name)) => {
                let muted = words.get(1) == Some(&"mute");
                let person = self
                    .voice
                    .people()
                    .into_iter()
                    .find(|p| p.name == name)
                    .ok_or("nobody of that name is in voice chat")?;
                self.voice.set_muted(person.uuid, muted);
            }
            (Some("volume"), Some(name)) => {
                let volume: f32 = words
                    .get(3)
                    .and_then(|v| v.parse().ok())
                    .ok_or("voice volume NAME 0..2")?;
                let person = self
                    .voice
                    .people()
                    .into_iter()
                    .find(|p| p.name == name)
                    .ok_or("nobody of that name is in voice chat")?;
                self.voice.set_volume(person.uuid, volume);
            }
            (Some("status"), _) => {}
            _ => {
                return Err(
                    "voice enable|disable|mic on|off|toggle|input [NAME]|voices V|tone HZ|mute NAME|unmute NAME|volume NAME V|status"
                        .into(),
                );
            }
        }
        let heard = self
            .voice
            .probe
            .as_ref()
            .map(|p| *p.heard.lock().unwrap_or_else(|e| e.into_inner()))
            .unwrap_or_default();
        let (mic_level, speaking) = self.voice.mic_level();
        Ok(Some(json!({
            "offered": self.voice_offered(),
            "connected": self.voice.connected(),
            "enabled": self.voice.enabled(),
            "mic": self.voice.mic_live(),
            "speaking": speaking,
            "mic_level": mic_level,
            "input": self.voice.mic_choice(),
            "voices": self.voice.voices_volume(),
            "people": self.voice.people().iter().map(|p| json!({
                "name": p.name,
                "enabled": p.enabled,
                "mic": p.mic_live,
                "volume": p.volume,
                "muted": p.muted,
                "talking": p.talking,
                "level": p.level,
                "delay_ms": p.delay_ms,
            })).collect::<Vec<_>>(),
            "heard": {
                "level": heard.level,
                "hz": heard.hz,
                "blocks": heard.blocks,
                "quiet_blocks": heard.quiet_blocks,
            },
            "lines": self.voice.lines,
        })))
    }
}
