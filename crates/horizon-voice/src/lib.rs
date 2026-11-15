//! Voice chat between the players of a world.
//!
//! No game code and nothing unsafe: the game gives it microphone samples and asks it for the
//! others' voices; in between are the codec ([`codec`]) and, as they come, the messages, the
//! microphone's clean-up, each speaker's buffer and the engine that ties them.
//!
//! One voice is 48 kHz mono in frames of 20 ms ([`FRAME`] samples), whatever the microphone
//! and the sound output run at.
#![deny(unsafe_code)]

pub mod codec;

/// The rate voices are coded at.
pub const RATE: u32 = 48_000;
/// Samples in one frame: 20 ms.
pub const FRAME: usize = 960;
