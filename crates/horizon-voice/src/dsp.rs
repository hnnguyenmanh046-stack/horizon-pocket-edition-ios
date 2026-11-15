//! What is done to the microphone before it is coded, one frame at a time, and the limiter
//! every voice passes on its way out.
//!
//! Little, on purpose (each step costs battery and may colour the voice): the rumble below
//! speech goes ([`HighPass`]), silence is found so that it is never sent ([`Gate`]), the level is
//! brought slowly toward everyone's ([`Agc`]), and nothing leaves over full scale
//! ([`soft_limit`]). On a phone's speaker without the system's echo canceller, the microphone
//! is turned down while the others talk ([`EchoGuard`]).
use crate::{FRAME, RATE};

/// The level speech is brought toward (RMS, of full scale 1): about -20 dB.
pub const TARGET: f32 = 0.1;

/// The RMS of `samples`.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// `x` as it is up to 0.8 of full scale, and bent smoothly toward [`CEILING`] above that:
/// loud is loud, never clipped.
pub fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.8;
    let size = x.abs();
    if size <= KNEE || !size.is_finite() {
        return if size.is_finite() { x } else { 0.0 };
    }
    let over = (size - KNEE) / (CEILING - KNEE);
    (KNEE + (CEILING - KNEE) * over.tanh()).copysign(x)
}

/// The most [`soft_limit`] lets through: just under full scale, whatever comes in.
pub const CEILING: f32 = 0.98;

/// A second-order high-pass: what is below `cutoff` (a hand on the phone, wind, the hum of a
/// room) goes, the voice stays.
#[derive(Debug, Clone)]
pub struct HighPass {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl HighPass {
    pub fn new(cutoff: f32) -> Self {
        let w = std::f32::consts::TAU * cutoff / RATE as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        Self {
            b: [
                (1.0 + cos) / 2.0 / a0,
                -(1.0 + cos) / a0,
                (1.0 + cos) / 2.0 / a0,
            ],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for s in samples {
            let x = *s;
            let y = self.b[0] * x + self.z[0];
            self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
            self.z[1] = self.b[2] * x - self.a[1] * y;
            *s = y;
        }
        // A filter fed nothing for long must not creep into denormals.
        if self.z[0].abs() < 1e-20 {
            self.z = [0.0; 2];
        }
    }
}

/// What the gate says of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing to send.
    Silent,
    /// Speech (or the short tail kept after it): send it.
    Speech,
    /// The speech is over: this frame isn't sent, the listeners are told it ended.
    End,
}

/// Finds speech by its level over the room's own: the room's level is followed (down at
/// once, up slowly), speech is what stands well above it, and it is held a moment after the
/// last sound so that words aren't cut between syllables.
#[derive(Debug, Clone)]
pub struct Gate {
    /// The room's level, as followed.
    floor: f32,
    open: bool,
    /// Frames left of the tail.
    hang: u32,
}

/// The tail kept after the last sound: 300 ms.
const HANG: u32 = 15;
/// The quietest speech found, whatever the room: about -48 dB.
const OPEN_AT: f32 = 0.004;
const STAY_AT: f32 = 0.0025;
/// Speech stands this far over the room to open the gate, and this far to keep it open.
const OPEN_OVER: f32 = 3.0;
const STAY_OVER: f32 = 1.8;
/// The room's level rises this much a frame toward a steady sound (2 dB a second: a fan that
/// starts is the room after a few seconds, a long sentence isn't).
const RISE: f32 = 1.005;
const FLOOR: std::ops::RangeInclusive<f32> = 0.0002..=0.05;

impl Default for Gate {
    fn default() -> Self {
        Self {
            floor: 0.002,
            open: false,
            hang: 0,
        }
    }
}

impl Gate {
    pub fn decide(&mut self, frame: &[f32]) -> Verdict {
        let level = rms(frame);
        if level < self.floor {
            self.floor = self.floor * 0.7 + level * 0.3;
        } else {
            self.floor *= RISE;
        }
        self.floor = self.floor.clamp(*FLOOR.start(), *FLOOR.end());
        let open_at = (self.floor * OPEN_OVER).max(OPEN_AT);
        let stay_at = (self.floor * STAY_OVER).max(STAY_AT);
        if level > open_at || (self.open && level > stay_at) {
            self.open = true;
            self.hang = HANG;
            return Verdict::Speech;
        }
        if !self.open {
            return Verdict::Silent;
        }
        if self.hang > 0 {
            self.hang -= 1;
            return Verdict::Speech;
        }
        self.open = false;
        Verdict::End
    }

    pub fn open(&self) -> bool {
        self.open
    }
}

/// Brings speech slowly toward [`TARGET`]: a quiet voice up, a loud one down, the same for
/// seconds at a time, so that the voice keeps its own rises and falls.
#[derive(Debug, Clone)]
pub struct Agc {
    gain: f32,
}

const GAIN: std::ops::RangeInclusive<f32> = 0.25..=8.0;

impl Default for Agc {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

impl Agc {
    /// Applies the gain to a frame of speech, and moves it by what the frame says.
    pub fn process(&mut self, frame: &mut [f32]) {
        let level = rms(frame);
        let from = self.gain;
        // A frame of near nothing (the tail after a word) says nothing about the voice.
        if level > 0.003 {
            let wanted = (TARGET / level).clamp(*GAIN.start(), *GAIN.end());
            // Down fast (a shout is loud at once), up slowly (a pause isn't a quiet voice).
            let pace = if wanted < self.gain { 0.1 } else { 0.02 };
            self.gain += (wanted - self.gain) * pace;
        }
        let step = (self.gain - from) / frame.len().max(1) as f32;
        for (i, s) in frame.iter_mut().enumerate() {
            *s *= from + step * i as f32;
        }
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }
}

/// Turns the microphone down while the others' voices come out of the phone's own speaker,
/// so they don't hear themselves back: what is used when the system's echo canceller isn't.
#[derive(Debug, Clone)]
pub struct EchoGuard {
    gain: f32,
}

/// The others are heard at this level or more: the microphone is held down (-24 dB).
const FAR_AT: f32 = 0.01;
const HELD: f32 = 0.063;

impl Default for EchoGuard {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

impl EchoGuard {
    /// `far` is how loud the others' voices are coming out now.
    pub fn process(&mut self, frame: &mut [f32], far: f32) {
        let wanted = if far >= FAR_AT { HELD } else { 1.0 };
        // Down at once (the first of their sound mustn't get through), back up over a
        // quarter of a second (the room rings on).
        if wanted < self.gain {
            self.gain = wanted;
        }
        let from = self.gain;
        self.gain += (wanted - self.gain) * 0.08;
        let step = (self.gain - from) / frame.len().max(1) as f32;
        for (i, s) in frame.iter_mut().enumerate() {
            *s *= from + step * i as f32;
        }
    }
}

/// The noise filter a player may turn on: a room's steady noise (a fan, traffic, a keyboard)
/// taken out of the microphone before anything else looks at it (RNNoise). Off unless asked
/// for: it costs more than the codec does, and a voice is a little less its own through it.
pub struct Denoise {
    state: Box<nnnoiseless::DenoiseState<'static>>,
    /// Its first frame fades in from nothing: not used.
    primed: bool,
}

impl Default for Denoise {
    fn default() -> Self {
        Self {
            state: nnnoiseless::DenoiseState::new(),
            primed: false,
        }
    }
}

impl Denoise {
    /// Filters a frame in place (two of the filter's own frames of 10 ms).
    pub fn process(&mut self, frame: &mut [f32; FRAME]) {
        const PART: usize = nnnoiseless::DenoiseState::FRAME_SIZE;
        // The filter works on samples as 16-bit ones count them.
        const SCALE: f32 = 32_768.0;
        let mut input = [0.0f32; PART];
        let mut output = [0.0f32; PART];
        for part in frame.chunks_exact_mut(PART) {
            for (to, from) in input.iter_mut().zip(part.iter()) {
                *to = from * SCALE;
            }
            self.state.process_frame(&mut output, &input);
            if !std::mem::replace(&mut self.primed, true) {
                part.fill(0.0);
                continue;
            }
            for (to, from) in part.iter_mut().zip(&output) {
                *to = if from.is_finite() { from / SCALE } else { 0.0 };
            }
        }
    }
}

/// The microphone's whole way to the coder.
pub struct Chain {
    high_pass: HighPass,
    denoise: Option<Denoise>,
    gate: Gate,
    agc: Agc,
    guard: EchoGuard,
    /// Leave the level as the microphone gives it (the system already levels it).
    pub level_as_is: bool,
}

impl Default for Chain {
    fn default() -> Self {
        Self {
            high_pass: HighPass::new(80.0),
            denoise: None,
            gate: Gate::default(),
            agc: Agc::default(),
            guard: EchoGuard::default(),
            level_as_is: false,
        }
    }
}

impl Chain {
    /// The way for a microphone whose level is left as the system gives it, or not, with
    /// the noise filter or without.
    pub fn new(level_as_is: bool, noise_filter: bool) -> Self {
        Self {
            level_as_is,
            denoise: noise_filter.then(Denoise::default),
            ..Self::default()
        }
    }

    /// What speech is multiplied by now, to bring it toward everyone's level.
    pub fn gain(&self) -> f32 {
        if self.level_as_is {
            1.0
        } else {
            self.agc.gain()
        }
    }

    /// Cleans `frame` in place and says whether it is speech. `far` is how loud the others'
    /// voices are coming out of this device's speaker now, when the microphone must be kept
    /// from hearing them (None: headphones, or the system sees to it).
    pub fn process(&mut self, frame: &mut [f32; FRAME], far: Option<f32>) -> Verdict {
        for s in frame.iter_mut() {
            if !s.is_finite() {
                *s = 0.0;
            }
        }
        self.high_pass.process(frame);
        if let Some(denoise) = &mut self.denoise {
            denoise.process(frame);
        }
        if let Some(far) = far {
            self.guard.process(frame, far);
        }
        let verdict = self.gate.decide(frame);
        if verdict == Verdict::Speech {
            if !self.level_as_is {
                self.agc.process(frame);
            }
            for s in frame.iter_mut() {
                *s = soft_limit(*s);
            }
        }
        verdict
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::tests::{amplitude_at, tone};

    fn noise(level: f32, frames: usize, seed: &mut u32) -> Vec<[f32; FRAME]> {
        (0..frames)
            .map(|_| {
                let mut frame = [0.0; FRAME];
                for s in &mut frame {
                    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    // Uniform noise of RMS `level`.
                    *s = ((*seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0) * level * 3f32.sqrt();
                }
                frame
            })
            .collect()
    }

    #[test]
    fn the_limiter_never_passes_full_scale_and_leaves_the_quiet_alone() {
        let mut last = -1.0;
        for i in 0..=4000 {
            let x = i as f32 / 100.0;
            let y = soft_limit(x);
            assert!(y <= CEILING && y >= last, "{x} gives {y}");
            assert_eq!(soft_limit(-x), -y);
            if x <= 0.8 {
                assert_eq!(y, x);
            }
            last = y;
        }
        assert_eq!(soft_limit(f32::NAN), 0.0);
        assert_eq!(soft_limit(f32::INFINITY), 0.0);
    }

    #[test]
    fn the_rumble_goes_and_the_voice_stays() {
        let mut high_pass = HighPass::new(80.0);
        // A steady push (DC) and a 20 Hz rumble, with a voice's 300 Hz on top.
        let mut samples: Vec<f32> = tone(300.0, 0.3, 50).concat();
        for (i, s) in samples.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            *s += 0.4 + 0.3 * (std::f32::consts::TAU * 20.0 * t).sin();
        }
        high_pass.process(&mut samples);
        let settled = &samples[RATE as usize / 4..];
        let mean = settled.iter().sum::<f32>() / settled.len() as f32;
        assert!(mean.abs() < 0.005, "DC left: {mean}");
        assert!(amplitude_at(settled, 20.0) < 0.03);
        assert!((amplitude_at(settled, 300.0) - 0.3).abs() < 0.02);
        // Nothing in, nothing out, for as long as it lasts.
        let mut nothing = vec![0.0; RATE as usize];
        high_pass.process(&mut nothing);
        assert!(nothing[RATE as usize / 2..].iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn silence_is_silent_speech_opens_at_once_and_is_held_a_moment_after() {
        let mut gate = Gate::default();
        for frame in tone(200.0, 0.0005, 100) {
            assert_eq!(gate.decide(&frame), Verdict::Silent);
        }
        let speech = tone(200.0, 0.1, 10);
        assert_eq!(gate.decide(&speech[0]), Verdict::Speech);
        for frame in &speech[1..] {
            assert_eq!(gate.decide(frame), Verdict::Speech);
        }
        // The tail: 300 ms more, then one end, then silence.
        let quiet = [0.0; FRAME];
        for _ in 0..HANG {
            assert_eq!(gate.decide(&quiet), Verdict::Speech);
        }
        assert_eq!(gate.decide(&quiet), Verdict::End);
        assert!(!gate.open());
        for _ in 0..50 {
            assert_eq!(gate.decide(&quiet), Verdict::Silent);
        }
        // A word again within the tail keeps the stretch whole.
        assert_eq!(gate.decide(&speech[0]), Verdict::Speech);
        for _ in 0..HANG - 1 {
            assert_eq!(gate.decide(&quiet), Verdict::Speech);
        }
        assert_eq!(gate.decide(&speech[0]), Verdict::Speech);
        for _ in 0..HANG {
            assert_eq!(gate.decide(&quiet), Verdict::Speech);
        }
        assert_eq!(gate.decide(&quiet), Verdict::End);
    }

    #[test]
    fn a_steady_noise_becomes_the_room_and_speech_over_it_is_still_found() {
        let mut gate = Gate::default();
        let mut seed = 5;
        // A fan starts: sent for a few seconds at most, then it is the room.
        let fan = noise(0.01, 600, &mut seed);
        let mut sent = 0;
        for (i, frame) in fan.iter().enumerate() {
            if gate.decide(frame) == Verdict::Speech {
                sent += 1;
                assert!(i < 400, "the fan is still sent after 8 seconds");
            }
        }
        assert!(sent < 400);
        assert!(!gate.open());
        // Speech over the fan opens it, and the fan alone closes it again.
        let mut speech = tone(250.0, 0.1, 20);
        for (frame, fan) in speech.iter_mut().zip(noise(0.01, 20, &mut seed)) {
            for (s, n) in frame.iter_mut().zip(fan) {
                *s += n;
            }
        }
        assert_eq!(gate.decide(&speech[0]), Verdict::Speech);
        for frame in &speech[1..] {
            assert_eq!(gate.decide(frame), Verdict::Speech);
        }
        let mut ended = false;
        for frame in noise(0.01, 100, &mut seed) {
            ended |= gate.decide(&frame) == Verdict::End;
        }
        assert!(ended && !gate.open());
    }

    #[test]
    fn a_quiet_voice_comes_up_a_loud_one_down_and_neither_jumps() {
        for said in [0.01f32, 0.03, 0.3, 0.9] {
            let mut agc = Agc::default();
            let mut last = 1.0;
            let mut out = Vec::new();
            for mut frame in tone(220.0, said * 2f32.sqrt(), 400) {
                agc.process(&mut frame);
                assert!((agc.gain() - last).abs() <= last * 0.15, "the gain jumped");
                last = agc.gain();
                out = frame.to_vec();
            }
            let level = rms(&out);
            let reachable = (TARGET / said).clamp(0.25, 8.0) * said;
            assert!(
                (level - reachable).abs() < reachable * 0.1,
                "{said} came out at {level}"
            );
        }
        // The tail after a word doesn't move it.
        let mut agc = Agc::default();
        for mut frame in tone(220.0, 0.05, 100) {
            agc.process(&mut frame);
        }
        let before = agc.gain();
        for _ in 0..200 {
            agc.process(&mut [0.0001; FRAME]);
        }
        assert_eq!(agc.gain(), before);
    }

    #[test]
    fn the_microphone_is_held_down_while_the_others_are_heard_and_comes_back() {
        let mut guard = EchoGuard::default();
        let mut frame = [0.5; FRAME];
        guard.process(&mut frame, 0.0);
        assert_eq!(frame[FRAME - 1], 0.5);
        for _ in 0..10 {
            frame = [0.5; FRAME];
            guard.process(&mut frame, 0.2);
        }
        assert!(frame[FRAME - 1] < 0.5 * HELD * 1.1);
        let mut back = 0;
        while frame[FRAME - 1] < 0.45 {
            frame = [0.5; FRAME];
            guard.process(&mut frame, 0.0);
            back += 1;
            assert!(back < 100);
        }
        // Not at once (the room still rings), and within a second.
        assert!((5..50).contains(&back), "back after {back} frames");
    }

    #[test]
    fn the_noise_filter_takes_a_rooms_noise_down_and_keeps_within_bounds() {
        let mut denoise = Denoise::default();
        let mut seed = 21;
        let mut left = 0.0;
        for mut frame in noise(0.05, 150, &mut seed) {
            denoise.process(&mut frame);
            assert!(frame.iter().all(|s| s.is_finite() && s.abs() <= 1.5));
            left = rms(&frame);
        }
        assert!(left < 0.05 * 0.7, "the noise is still at {left}");
        // Silence in is silence out.
        let mut quiet = [0.0; FRAME];
        for _ in 0..10 {
            denoise.process(&mut quiet);
        }
        assert!(rms(&quiet) < 0.001);
        // In the chain it comes before the gate: a noisy room isn't sent.
        let mut chain = Chain::new(false, true);
        let mut sent = 0;
        for mut frame in noise(0.02, 300, &mut seed) {
            sent += usize::from(chain.process(&mut frame, None) == Verdict::Speech);
        }
        assert!(sent < 100, "{sent} frames of a noisy room were sent");
    }

    #[test]
    fn the_whole_way_gives_speech_at_its_level_under_full_scale() {
        let mut chain = Chain::default();
        let mut seed = 11;
        for mut frame in noise(0.001, 100, &mut seed) {
            assert_eq!(chain.process(&mut frame, None), Verdict::Silent);
        }
        let mut peak = 0f32;
        let mut level = 0.0;
        // A shout into the microphone, clipped by it.
        for mut frame in tone(180.0, 1.5, 300) {
            for s in &mut frame {
                *s = s.clamp(-1.0, 1.0);
            }
            frame[7] = f32::NAN;
            assert_eq!(chain.process(&mut frame, None), Verdict::Speech);
            peak = frame.iter().fold(peak, |p, s| p.max(s.abs()));
            level = rms(&frame);
        }
        assert!(peak < 1.0, "over full scale: {peak}");
        assert!(level < 0.3, "the shout stayed at {level}");
        // With the others heard on the speaker, their echo doesn't open the gate.
        let mut chain = Chain::default();
        for mut frame in tone(300.0, 0.05, 100) {
            assert_eq!(chain.process(&mut frame, Some(0.3)), Verdict::Silent);
        }
        // As the microphone gives it, when asked.
        let mut chain = Chain::new(true, false);
        let mut frame = tone(300.0, 0.05, 1)[0];
        chain.process(&mut frame, None);
        assert!(amplitude_at(&frame[FRAME / 2..], 300.0) < 0.06);
    }
}
