use std::fmt;
use std::sync::Arc;

use super::ring::Ring;

#[derive(Debug)]
pub enum AudioError {
    NoDevice,
    Config(String),
    Device(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoDevice => write!(f, "no default output device"),
            AudioError::Config(m) => write!(f, "unusable output config: {m}"),
            AudioError::Device(m) => write!(f, "audio device: {m}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// The device, and nothing else. Everything about occupancy, rate and muting belongs to the
/// ring it drains, which is what lets the emulator and the UI both write to it without
/// either of them owning the hardware.
pub trait AudioSink: Send {
    /// The rate is a preference. A device that will not take it opens at its own, which the
    /// ring then reports and the resampler converts to.
    fn open(&mut self, sample_rate: u32) -> Result<(), AudioError>;
    /// Start opening the device without making the caller wait for a driver or mixer.
    /// Returns `true` when completion must be collected with `poll_open`. The default keeps
    /// small/test sinks synchronous while hardware sinks can move their slow probe off boot.
    fn open_async(&mut self, sample_rate: u32) -> Result<bool, AudioError> {
        self.open(sample_rate)?;
        Ok(false)
    }
    /// Collect the result of an asynchronous open, if it has completed.
    fn poll_open(&mut self) -> Option<Result<(), AudioError>> {
        None
    }
    /// Close the hardware synchronously. An open H700 PCM keeps the speaker amp biased,
    /// which is a hiss with the panel already dark — so this runs before suspend, doze, and
    /// power off, not only when the sink is being replaced.
    fn close(&mut self);
    fn ring(&self) -> Arc<Ring>;
}
