//! The microphone, for voice chat, where the game's sound library reads it (cpal): a desktop
//! and Android. On an iPhone the session's own unit is used instead
//! (`horizon_gamecontroller::voice`), with the system's routes and echo canceller.
//!
//! The samples go to the caller as the device gives them, interleaved floats with their
//! channel count and rate; what is made of them is voice chat's (`horizon-voice`).

/// The microphone's samples: interleaved, with their channel count and rate. Called on the
/// device's thread: it must only copy.
pub type Samples = Box<dyn FnMut(&[f32], usize, u32) + Send>;

/// A running capture: dropping it stops the microphone.
pub struct Capture {
    #[cfg(feature = "audio")]
    _stream: cpal::Stream,
}

/// The microphones there are, as the system names them.
#[cfg(feature = "audio")]
pub fn input_devices() -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let mut names = Vec::new();
    if let Ok(devices) = cpal::default_host().input_devices() {
        for device in devices {
            if let Ok(name) = device.name()
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
    }
    names
}

#[cfg(not(feature = "audio"))]
pub fn input_devices() -> Vec<String> {
    Vec::new()
}

/// Starts the microphone named `name` (the system's default for "", or when that one is
/// gone).
#[cfg(feature = "audio")]
pub fn open(name: &str, samples: Samples) -> Result<Capture, String> {
    use cpal::{
        SampleFormat as F,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    };
    let host = cpal::default_host();
    let named = (!name.is_empty())
        .then(|| {
            host.input_devices()
                .ok()?
                .find(|d| d.name().is_ok_and(|n| n == name))
        })
        .flatten();
    let device = named
        .or_else(|| host.default_input_device())
        .ok_or("no microphone was found")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("the microphone's format: {e}"))?;
    let stream = match config.sample_format() {
        F::F32 => build::<f32>(&device, &config, samples),
        F::F64 => build::<f64>(&device, &config, samples),
        F::I16 => build::<i16>(&device, &config, samples),
        F::I32 => build::<i32>(&device, &config, samples),
        F::I8 => build::<i8>(&device, &config, samples),
        F::U16 => build::<u16>(&device, &config, samples),
        F::U32 => build::<u32>(&device, &config, samples),
        F::U8 => build::<u8>(&device, &config, samples),
        other => return Err(format!("the microphone's sample format {other}")),
    }
    .map_err(|e| format!("the microphone couldn't be opened: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("the microphone couldn't be started: {e}"))?;
    Ok(Capture { _stream: stream })
}

#[cfg(not(feature = "audio"))]
pub fn open(name: &str, samples: Samples) -> Result<Capture, String> {
    let _ = (name, samples);
    Err("this build has no sound".into())
}

#[cfg(feature = "audio")]
fn build<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    mut samples: Samples,
) -> std::result::Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    use cpal::{Sample, traits::DeviceTrait};
    let channels = usize::from(config.channels());
    let rate = config.sample_rate().0;
    let mut floats: Vec<f32> = Vec::new();
    device.build_input_stream(
        &config.config(),
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            floats.clear();
            floats.extend(data.iter().map(|s| f32::from_sample(*s)));
            samples(&floats, channels, rate);
        },
        |e| log::warn!("Horizon: the microphone's stream: {e}"),
        None,
    )
}
