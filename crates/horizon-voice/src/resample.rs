//! Changing a sound's rate: the microphone's to the codec's 48 kHz, and the voices' 48 kHz to
//! whatever the sound output runs at. On an iPhone both are 48 kHz already and nothing here
//! runs ([`Resampler::same`]).
//!
//! A windowed sinc, 24 samples each way: nothing a voice has is lost or folded back.

/// Samples of the input on each side of an output sample.
const HALF: usize = 24;
/// Positions between two input samples the filter is worked out for (the rest is drawn
/// between two of them).
const PHASES: usize = 128;

/// A sound going from one rate to another, a piece at a time.
#[derive(Debug, Clone)]
pub struct Resampler {
    from: u32,
    to: u32,
    /// The filter at each phase: `PHASES + 1` rows of `2 * HALF`.
    table: Vec<f32>,
    /// Input not used up yet, with the `HALF - 1` samples before it.
    held: Vec<f32>,
    /// Where the next output sample falls in `held`.
    at: f64,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        let (from, to) = (from.max(1), to.max(1));
        // Going down, what the lower rate can't carry is filtered out first.
        let cutoff = (f64::from(to) / f64::from(from)).min(1.0);
        let mut table = Vec::with_capacity((PHASES + 1) * 2 * HALF);
        for phase in 0..=PHASES {
            let frac = phase as f64 / PHASES as f64;
            let row: Vec<f64> = (0..2 * HALF)
                .map(|tap| {
                    let x = tap as f64 - (HALF - 1) as f64 - frac;
                    let u = x / HALF as f64;
                    // Blackman's window over the sinc.
                    let window = 0.42
                        + 0.5 * (std::f64::consts::PI * u).cos()
                        + 0.08 * (std::f64::consts::TAU * u).cos();
                    let t = std::f64::consts::PI * x * cutoff;
                    let sinc = if t.abs() < 1e-9 { 1.0 } else { t.sin() / t };
                    cutoff * sinc * window.max(0.0)
                })
                .collect();
            // A steady level in is the same level out, at every phase.
            let sum: f64 = row.iter().sum();
            table.extend(row.iter().map(|c| (c / sum) as f32));
        }
        Self {
            from,
            to,
            table,
            held: vec![0.0; HALF - 1],
            at: (HALF - 1) as f64,
        }
    }

    /// The two rates are one: [`Self::process`] passes the sound through.
    pub fn same(&self) -> bool {
        self.from == self.to
    }

    pub fn rates(&self) -> (u32, u32) {
        (self.from, self.to)
    }

    /// Takes `input` (at the first rate) and adds to `out` what it makes at the second. What
    /// can't be made yet (it needs the samples to come) is made with the next piece.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.same() {
            out.extend_from_slice(input);
            return;
        }
        self.held.extend_from_slice(input);
        let step = f64::from(self.from) / f64::from(self.to);
        let taps = 2 * HALF;
        loop {
            let whole = self.at as usize;
            // The filter reaches HALF samples past the one before the output's place.
            if whole + HALF >= self.held.len() {
                break;
            }
            let frac = (self.at - whole as f64) * PHASES as f64;
            let phase = (frac as usize).min(PHASES - 1);
            let blend = (frac - phase as f64) as f32;
            let low = &self.table[phase * taps..(phase + 1) * taps];
            let high = &self.table[(phase + 1) * taps..(phase + 2) * taps];
            let window = &self.held[whole + 1 - HALF..whole + 1 + HALF];
            let mut sample = 0.0f32;
            for tap in 0..taps {
                sample += window[tap] * (low[tap] + (high[tap] - low[tap]) * blend);
            }
            out.push(sample);
            self.at += step;
        }
        // Keep only what the next output still reaches back to.
        let used = (self.at as usize + 1).saturating_sub(HALF);
        if used > 0 {
            self.held.drain(..used.min(self.held.len()));
            self.at -= used as f64;
        }
    }

    /// Forgets the sound so far: what comes next starts from silence.
    pub fn reset(&mut self) {
        self.held.clear();
        self.held.resize(HALF - 1, 0.0);
        self.at = (HALF - 1) as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, rate: u32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    fn amplitude(samples: &[f32], hz: f32, rate: u32) -> f32 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, s) in samples.iter().enumerate() {
            let phase = std::f64::consts::TAU * f64::from(hz) * i as f64 / f64::from(rate);
            re += f64::from(*s) * phase.cos();
            im += f64::from(*s) * phase.sin();
        }
        (2.0 * (re * re + im * im).sqrt() / samples.len() as f64) as f32
    }

    #[test]
    fn the_same_rate_is_the_same_sound() {
        let mut same = Resampler::new(48_000, 48_000);
        assert!(same.same());
        let input = sine(440.0, 48_000, 0.1);
        let mut out = Vec::new();
        same.process(&input, &mut out);
        assert_eq!(out, input);
    }

    #[test]
    fn a_voice_keeps_its_pitch_and_level_at_another_rate() {
        for (from, to) in [
            (44_100, 48_000),
            (48_000, 44_100),
            (16_000, 48_000),
            (48_000, 16_000),
            (24_000, 48_000),
            (48_000, 96_000),
        ] {
            let mut resampler = Resampler::new(from, to);
            assert!(!resampler.same());
            for hz in [100.0, 1_000.0, 3_000.0] {
                resampler.reset();
                let mut out = Vec::new();
                resampler.process(&sine(hz, from, 1.0), &mut out);
                let wanted = to as usize;
                assert!(
                    out.len().abs_diff(wanted) <= 2 * HALF * to as usize / from as usize + 2,
                    "{from} to {to}: {} samples for a second",
                    out.len()
                );
                let settled = &out[to as usize / 10..];
                let level = amplitude(settled, hz, to);
                assert!(
                    (level - 0.5).abs() < 0.01,
                    "{from} to {to} at {hz} Hz: {level}"
                );
                assert!(settled.iter().all(|s| s.abs() < 0.52));
            }
        }
    }

    #[test]
    fn what_the_lower_rate_cant_carry_doesnt_fold_back() {
        // 14 kHz can't be carried at 16 kHz (8 kHz at most): it must be gone, not heard as 2.
        let mut resampler = Resampler::new(48_000, 16_000);
        let mut out = Vec::new();
        resampler.process(&sine(14_000.0, 48_000, 1.0), &mut out);
        let settled = &out[1_600..];
        assert!(amplitude(settled, 2_000.0, 16_000) < 0.02);
        assert!(settled.iter().all(|s| s.abs() < 0.05));
    }

    #[test]
    fn pieces_of_any_size_make_the_same_sound_as_one() {
        let input = sine(700.0, 44_100, 0.5);
        let mut whole = Vec::new();
        Resampler::new(44_100, 48_000).process(&input, &mut whole);
        let mut pieces = Vec::new();
        let mut resampler = Resampler::new(44_100, 48_000);
        let mut at = 0;
        for size in [1usize, 7, 441, 3, 1024, 2, 513].iter().cycle() {
            if at >= input.len() {
                break;
            }
            let end = (at + size).min(input.len());
            resampler.process(&input[at..end], &mut pieces);
            at = end;
        }
        assert_eq!(pieces.len(), whole.len());
        for (a, b) in pieces.iter().zip(&whole) {
            assert!((a - b).abs() < 1e-5);
        }
        // A steady level stays that level, with no ripple from the filter's phases.
        let mut flat = Vec::new();
        Resampler::new(44_100, 48_000).process(&[0.25; 44_100], &mut flat);
        assert!(flat[4_800..].iter().all(|s| (s - 0.25).abs() < 1e-4));
    }
}
