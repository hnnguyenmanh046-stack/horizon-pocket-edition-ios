//! One speaker's voice between the connection and the sound output.
//!
//! The connection loses nothing and keeps order, but it delivers in bursts (a frame every
//! 20 ms leaves, three arrive together), and the sound output asks for sound at its own steady
//! pace. So a few frames are held before a stretch of speech starts to play ([`Jitter::target`]):
//! as few as the connection's bursts allow, more when they grow, fewer again when they calm.
//! When too much is held (the connection stalled, then gave everything at once) the oldest is
//! skipped, so the voice is never far behind; when nothing is there in time, the codec carries
//! the voice on for a moment ([`crate::codec::Decoder::conceal`]).
//!
//! Frames are decoded when the output asks for them, never ahead.
use crate::{
    FRAME, RATE,
    codec::{Decoder, MAX_PACKET},
    dsp::rms,
    wire::{END, START},
};
use std::{collections::VecDeque, time::Duration};

/// Frames held before speech plays, at least: 40 ms.
pub const MIN_TARGET: usize = 2;
/// And at most: 400 ms.
pub const MAX_TARGET: usize = 20;
/// More than this held (a second) is a stall's backlog: all but the target is dropped.
const MAX_HELD: usize = 50;
/// Frames over the target before the oldest are skipped.
const SLACK: usize = 3;
/// Frames the codec carries a voice on for when nothing came.
const CONCEAL: u32 = 3;
/// Frames played without a gap before the target may come down by one: a second.
const CALM: u32 = 50;
/// The arrivals the bursts are measured over: three seconds of speech.
const WINDOW: usize = 150;
/// A stretch's first frames come together (the sender keeps a moment from before the gate
/// opened): arrivals are timed from this one on.
const TIMED_FROM: u32 = 3;
const FRAME_MS: i64 = 20;

struct Packet {
    flags: u8,
    len: usize,
    data: [u8; MAX_PACKET],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nobody speaks.
    Idle,
    /// A stretch began: holding frames until there are enough (`waited` frames so far).
    Filling {
        waited: usize,
    },
    Playing,
}

pub struct Jitter {
    decoder: Decoder,
    held: VecDeque<Packet>,
    state: State,
    target: usize,
    /// The frame being played, and how far into it.
    pcm: [f32; FRAME],
    at: usize,
    concealed: u32,
    calm: u32,
    /// Skipping ahead, until no more is held than the target.
    catching: bool,
    /// The last frame taken in.
    last: Option<u16>,
    /// Frames taken in since the stretch began.
    since_start: u32,
    /// When the frame arrivals are timed from came, and its number.
    base: Option<(Duration, u16)>,
    /// How late each arrival was against that, in ms.
    late: VecDeque<i64>,
    level: f32,
    skipped: u64,
}

impl Jitter {
    pub fn new() -> crate::codec::Result<Self> {
        Ok(Self {
            decoder: Decoder::new()?,
            held: VecDeque::with_capacity(MAX_HELD + 1),
            state: State::Idle,
            target: MIN_TARGET,
            pcm: [0.0; FRAME],
            at: FRAME,
            concealed: 0,
            calm: 0,
            catching: false,
            last: None,
            since_start: 0,
            base: None,
            late: VecDeque::with_capacity(WINDOW + 1),
            level: 0.0,
            skipped: 0,
        })
    }

    /// Takes in a frame as it arrives, at `now` (any clock that only goes forward).
    pub fn push(&mut self, seq: u16, flags: u8, opus: &[u8], now: Duration) {
        if opus.len() > MAX_PACKET {
            return;
        }
        let start = flags & START != 0;
        // One again, or one from before (the numbers go round): not played twice.
        if let Some(last) = self.last
            && !start
            && matches!(seq.wrapping_sub(last), 0 | 0x8000..)
        {
            return;
        }
        self.last = Some(seq);
        if start {
            self.since_start = 0;
            self.base = None;
        }
        self.since_start += 1;
        if self.since_start == TIMED_FROM {
            self.base = Some((now, seq));
        } else if let Some((at, first)) = self.base {
            let due = i64::from(seq.wrapping_sub(first)) * FRAME_MS;
            let came = i64::try_from(now.saturating_sub(at).as_millis()).unwrap_or(i64::MAX);
            self.late.push_back(came.saturating_sub(due));
            if self.late.len() > WINDOW {
                self.late.pop_front();
            }
            // Bursts grew: hold more from now on.
            self.target = self.target.max(self.wanted());
        }
        let mut data = [0u8; MAX_PACKET];
        data[..opus.len()].copy_from_slice(opus);
        self.held.push_back(Packet {
            flags,
            len: opus.len(),
            data,
        });
        // A stall's whole backlog at once: what is old is gone, the voice goes on from now.
        if self.held.len() > MAX_HELD {
            while self.held.len() > self.target {
                self.held.pop_front();
                self.skipped += 1;
            }
            let _ = self.decoder.reset();
        }
        if self.state == State::Idle {
            self.state = State::Filling { waited: 0 };
        }
    }

    /// The frames to hold for the bursts seen lately.
    fn wanted(&self) -> usize {
        let (Some(most), Some(least)) = (self.late.iter().max(), self.late.iter().min()) else {
            return MIN_TARGET;
        };
        let spread = usize::try_from(most.saturating_sub(*least)).unwrap_or(0);
        (spread.div_ceil(FRAME_MS as usize) + 1).clamp(MIN_TARGET, MAX_TARGET)
    }

    /// Fills `out` with this speaker's voice at 48 kHz (silence when they don't speak).
    pub fn pull(&mut self, out: &mut [f32]) {
        let mut written = 0;
        while written < out.len() {
            if self.at >= FRAME {
                if self.idle() {
                    out[written..].fill(0.0);
                    return;
                }
                self.next_frame();
            }
            let n = (FRAME - self.at).min(out.len() - written);
            out[written..written + n].copy_from_slice(&self.pcm[self.at..self.at + n]);
            self.at += n;
            written += n;
        }
    }

    fn silence(&mut self) {
        self.pcm = [0.0; FRAME];
        self.level = 0.0;
    }

    /// Makes the next 20 ms to play.
    fn next_frame(&mut self) {
        self.at = 0;
        loop {
            match self.state {
                State::Idle => {
                    if self.held.is_empty() {
                        return self.silence();
                    }
                    self.state = State::Filling { waited: 0 };
                }
                State::Filling { waited } => {
                    // Enough held, or the whole stretch is here already, or waited long
                    // enough for what there is.
                    let ended = self.held.back().is_some_and(|p| p.flags & END != 0);
                    if self.held.len() >= self.target || ended || waited >= self.target + 2 {
                        self.state = if self.held.is_empty() {
                            State::Idle
                        } else {
                            State::Playing
                        };
                        continue;
                    }
                    self.state = State::Filling { waited: waited + 1 };
                    return self.silence();
                }
                State::Playing => return self.play(),
            }
        }
    }

    fn play(&mut self) {
        // Too far behind: skip the oldest (decoded all the same, so the codec keeps its
        // thread), two a frame at most, until no more is held than is meant to be.
        if self.held.len() > self.target + SLACK {
            self.catching = true;
        } else if self.held.len() <= self.target {
            self.catching = false;
        }
        let mut skipped = 0;
        while self.catching && self.held.len() > self.target && skipped < 2 {
            if let Some(packet) = self.held.pop_front() {
                self.decode(&packet);
            }
            skipped += 1;
            self.skipped += 1;
        }
        let Some(packet) = self.held.pop_front() else {
            // Nothing came in time: the codec carries the voice on for a moment, and more
            // is held from now on; then silence until frames come again.
            if self.concealed < CONCEAL {
                if self.concealed == 0 {
                    self.target = (self.target + 1).min(MAX_TARGET);
                    self.calm = 0;
                }
                self.concealed += 1;
                if self.decoder.conceal(&mut self.pcm).is_err() {
                    return self.silence();
                }
                self.level = rms(&self.pcm);
                return;
            }
            self.state = State::Filling { waited: 0 };
            return self.silence();
        };
        self.concealed = 0;
        self.calm += 1;
        if self.calm >= CALM {
            self.calm = 0;
            // The bursts calmed: hold less, half the way down at a time (the catch-up
            // brings the voice that much closer).
            let wanted = self.wanted();
            if wanted < self.target {
                self.target -= ((self.target - wanted) / 2).max(1);
                self.catching = true;
            }
        }
        self.decode(&packet);
        self.level = rms(&self.pcm);
        if packet.flags & END != 0 {
            self.state = State::Idle;
        }
    }

    /// Decodes `packet` into the frame being played.
    fn decode(&mut self, packet: &Packet) {
        if packet.flags & START != 0 {
            let _ = self.decoder.reset();
        }
        if packet.len == 0
            || self
                .decoder
                .decode(&packet.data[..packet.len], &mut self.pcm)
                .is_err()
        {
            self.pcm = [0.0; FRAME];
        }
    }

    /// Nothing to play and nothing held: [`Self::pull`] would give silence.
    pub fn idle(&self) -> bool {
        self.state == State::Idle && self.held.is_empty() && self.at >= FRAME
    }

    /// The speaker's voice is playing now.
    pub fn talking(&self) -> bool {
        self.state == State::Playing
    }

    /// The level of what was last played (RMS, 0 to 1).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Frames held before speech plays, now.
    pub fn target(&self) -> usize {
        self.target
    }

    /// How far behind the speaker the voice plays, from its arrival here: what is held.
    pub fn delay_ms(&self) -> u32 {
        let samples = self.held.len() * FRAME + FRAME.saturating_sub(self.at);
        (samples as u64 * 1000 / u64::from(RATE)) as u32
    }

    /// Frames skipped to keep up, since the start.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// Forgets everything held and playing (the speaker was muted, or left).
    pub fn clear(&mut self) {
        self.held.clear();
        self.state = State::Idle;
        self.at = FRAME;
        self.concealed = 0;
        self.catching = false;
        self.level = 0.0;
        self.last = None;
        self.base = None;
        self.since_start = 0;
        let _ = self.decoder.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{
        Encoder,
        tests::{amplitude_at, tone},
    };

    /// A speaker, a connection that delivers as `arrive` says, and a listener's output asking
    /// for 20 ms every 20 ms.
    struct Line {
        jitter: Jitter,
        /// (when it arrives, seq, flags, coded frame), in sending order.
        sent: VecDeque<(Duration, u16, u8, Vec<u8>)>,
        now: Duration,
        heard: Vec<f32>,
        /// The delay after each frame played.
        delays: Vec<u32>,
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    impl Line {
        /// `frames` frames of a tone sent from `from` ms on, one every 20 ms, the first
        /// marked a start and (with `end`) an end after the last; each arrives when
        /// `arrive(its sending time in ms)` says.
        fn speak(
            &mut self,
            encoder: &mut Encoder,
            first_seq: u16,
            from: u64,
            frames: usize,
            end: bool,
            arrive: impl Fn(u64) -> u64,
        ) {
            for (i, frame) in tone(440.0, 0.5, frames).iter().enumerate() {
                let mut packet = [0u8; MAX_PACKET];
                let n = encoder.encode(frame, &mut packet).unwrap();
                let sent = from + i as u64 * 20;
                self.sent.push_back((
                    ms(arrive(sent)),
                    first_seq.wrapping_add(i as u16),
                    if i == 0 { START } else { 0 },
                    packet[..n].to_vec(),
                ));
            }
            if end {
                let sent = from + frames as u64 * 20;
                self.sent.push_back((
                    ms(arrive(sent)),
                    first_seq.wrapping_add(frames as u16),
                    END,
                    Vec::new(),
                ));
            }
        }

        /// Plays until `until` ms.
        fn run(&mut self, until: u64) {
            while self.now < ms(until) {
                while self.sent.front().is_some_and(|(at, ..)| *at <= self.now) {
                    let (_, seq, flags, opus) = self.sent.pop_front().unwrap();
                    self.jitter.push(seq, flags, &opus, self.now);
                }
                let mut out = [0.0; FRAME];
                self.jitter.pull(&mut out);
                self.heard.extend_from_slice(&out);
                self.delays.push(self.jitter.delay_ms());
                self.now += ms(20);
            }
        }

        /// The frames heard between two times that were silent.
        fn gaps(&self, from: u64, to: u64) -> usize {
            self.heard[from as usize * 48..to as usize * 48]
                .chunks(FRAME)
                .filter(|frame| rms(frame) < 0.01)
                .count()
        }
    }

    fn line() -> (Line, Encoder) {
        (
            Line {
                jitter: Jitter::new().unwrap(),
                sent: VecDeque::new(),
                now: Duration::ZERO,
                heard: Vec::new(),
                delays: Vec::new(),
            },
            Encoder::new().unwrap(),
        )
    }

    #[test]
    fn a_steady_connection_plays_at_once_whole_and_close_behind() {
        let (mut line, mut encoder) = line();
        line.speak(&mut encoder, 0, 100, 200, true, |sent| sent + 30);
        line.run(100);
        assert!(line.jitter.idle());
        line.run(4_500);
        // Playing within 100 ms of the first arrival, then no gap until the end.
        assert_eq!(line.gaps(240, 4_100), 0, "gaps in a steady voice");
        assert!(amplitude_at(&line.heard[48 * 400..48 * 4_000], 440.0) > 0.4);
        // Never far behind: the two frames held and the one playing.
        assert!(line.delays.iter().all(|d| *d <= 80), "{:?}", line.delays);
        assert_eq!(line.jitter.target(), MIN_TARGET);
        // It ended: nothing more, and nothing held.
        assert!(line.jitter.idle() && !line.jitter.talking());
        assert_eq!(line.gaps(4_300, 4_500), 10);
        assert_eq!(line.jitter.skipped(), 0);
    }

    #[test]
    fn bursts_are_ridden_out_after_the_first_and_no_further_behind_than_they_are() {
        let (mut line, mut encoder) = line();
        // Three frames together every 60 ms (a relay that works in ticks).
        line.speak(&mut encoder, 500, 0, 500, true, |sent| sent / 60 * 60 + 60);
        line.run(10_500);
        // The first second finds out how bursty it is; after that the voice is whole.
        assert_eq!(line.gaps(1_500, 10_000), 0);
        assert!(line.jitter.target() <= 5, "target {}", line.jitter.target());
        assert!(
            line.delays[100..].iter().all(|d| *d <= 180),
            "{:?}",
            &line.delays[100..]
        );
    }

    #[test]
    fn after_a_stall_the_voice_skips_ahead_and_then_comes_close_again() {
        let (mut line, mut encoder) = line();
        // Steady for 2 s, then nothing arrives for 2 s and it all comes at once, then
        // steady again.
        line.speak(&mut encoder, 0, 0, 1_000, false, |sent| {
            if (2_000..4_000).contains(&sent) {
                4_000
            } else {
                sent + 30
            }
        });
        line.run(2_500);
        // The stall: the voice carried on for a moment, then silence, not noise.
        assert!(line.gaps(2_200, 2_500) > 5);
        line.run(20_000);
        // Never more than the most held behind, and within half a second of the stall's end
        // no more than half a second behind.
        let most = line.delays.iter().max().unwrap();
        assert!(*most <= (MAX_HELD as u32 + 2) * 20, "{most} ms behind");
        assert!(line.delays[225..].iter().all(|d| *d <= 500));
        assert!(line.jitter.skipped() > 40);
        // Whole again right after, and close again once it has been calm a while.
        assert_eq!(line.gaps(4_200, 19_900), 0);
        assert!(line.delays[900..].iter().all(|d| *d <= 80));
        assert_eq!(line.jitter.target(), MIN_TARGET);
    }

    #[test]
    fn stretches_of_speech_start_and_end_and_a_voice_that_stops_dead_goes_quiet() {
        let (mut line, mut encoder) = line();
        line.speak(&mut encoder, 0, 0, 50, true, |sent| sent + 20);
        // The next stretch, a second after, with numbers that go round.
        line.speak(&mut encoder, 65_530, 2_000, 50, true, |sent| sent + 20);
        line.run(1_900);
        assert!(line.jitter.idle());
        assert!(amplitude_at(&line.heard[48 * 200..48 * 900], 440.0) > 0.4);
        line.run(3_500);
        assert!(line.jitter.idle());
        assert!(amplitude_at(&line.heard[48 * 2_200..48 * 2_900], 440.0) > 0.4);
        assert_eq!(line.gaps(2_200, 2_900), 0);
        // A stretch with no end (its speaker's game went away): carried on for a moment,
        // then quiet, and idle again.
        line.speak(&mut encoder, 100, 4_000, 20, false, |sent| sent + 20);
        line.run(6_000);
        assert!(line.jitter.idle(), "still waiting for a voice that left");
        assert_eq!(line.gaps(4_700, 6_000), 65);
    }

    #[test]
    fn what_cant_be_played_is_dropped_and_nothing_breaks() {
        let (mut line, mut encoder) = line();
        // Too long, a repeat, one from before, and bytes that are no frame.
        line.jitter.push(1, START, &[0u8; MAX_PACKET + 1], ms(0));
        assert!(line.jitter.idle());
        line.speak(&mut encoder, 10, 0, 10, false, |sent| sent);
        line.run(100);
        let held = line.jitter.delay_ms();
        line.jitter.push(14, 0, &[1, 2, 3], ms(100));
        line.jitter.push(3, 0, &[1, 2, 3], ms(100));
        assert_eq!(line.jitter.delay_ms(), held);
        line.jitter.push(20, 0, &[0xff; 50], ms(100));
        line.jitter.push(21, 0, &[], ms(100));
        line.run(1_000);
        assert!(line.heard.iter().all(|s| s.is_finite() && s.abs() <= 1.5));
        // Cleared (muted, or gone): nothing left.
        line.speak(&mut encoder, 30, 1_000, 10, false, |sent| sent);
        line.run(1_100);
        line.jitter.clear();
        assert!(line.jitter.idle() && line.jitter.delay_ms() == 0);
        let mut out = [1.0; 100];
        line.jitter.pull(&mut out);
        assert_eq!(out, [0.0; 100]);
    }
}
