use thiserror::Error;

/// Hardware audio configuration.
#[derive(Debug, Clone)]
pub struct HardwareConfig {
    /// Device name to open, or `None` for the system default.
    pub device_name: Option<String>,
    /// Target output latency in milliseconds.
    pub latency_ms: f64,
    /// If true, open both input and output streams (duplex).
    pub duplex: bool,
    /// Number of output channels to request.
    pub n_channels: usize,
    /// Sample rate to request from the hardware driver. Schisma's instrument
    /// supports rates through 384 kHz; the selected device must advertise the
    /// requested rate.
    pub sample_rate: f64,
    /// Block size (frames per callback).
    pub block_size: usize,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            latency_ms: 10.0,
            duplex: false,
            n_channels: 2,
            sample_rate: 48000.0,
            block_size: 256,
        }
    }
}

/// Hardware audio host (CPAL-backed).
///
/// The `realtime` feature must be enabled to use this type.
/// When disabled, the type is still defined but `run()` returns an error.
pub struct HardwareHost {
    pub config: HardwareConfig,
}

/// A running hardware stream. Dropping this value stops audio.
pub struct HardwareStream {
    #[cfg(feature = "realtime")]
    _stream: cpal::Stream,
    device_name: String,
    sample_rate: u32,
    channels: u16,
}

impl HardwareStream {
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
}

impl HardwareHost {
    pub fn new(config: HardwareConfig) -> Self {
        Self { config }
    }

    /// Open the audio stream and begin processing.
    ///
    /// The provided callback runs on the audio thread.
    /// It receives an interleaved IEEE-754 32-bit float `&mut [f32]` buffer.
    /// It must not allocate. All external communication uses lock-free queues.
    ///
    /// This function blocks until the stream is stopped (e.g., via Ctrl+C).
    pub fn run<F>(&self, _callback: F) -> Result<(), HardwareError>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        self.run_for(_callback, None)
    }

    /// Open the stream for an optional bounded duration.
    ///
    /// A bounded run is intended for unattended device-soak automation. An
    /// unbounded run retains the interactive behavior of [`Self::run`].
    pub fn run_for<F>(
        &self,
        callback: F,
        duration: Option<std::time::Duration>,
    ) -> Result<(), HardwareError>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        let stream = self.open(callback)?;
        eprintln!(
            "→ Streaming to \"{}\" @ {}Hz, {} ch, {} block (Ctrl+C to stop)",
            stream.device_name(),
            stream.sample_rate(),
            stream.channels(),
            self.config.block_size,
        );
        if let Some(duration) = duration {
            std::thread::sleep(duration);
        } else {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        drop(stream);
        eprintln!("→ Stream stopped.");
        Ok(())
    }

    /// Open and start a non-blocking output stream.
    pub fn open<F>(&self, callback: F) -> Result<HardwareStream, HardwareError>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        #[cfg(not(feature = "realtime"))]
        {
            let _ = callback;
            Err(HardwareError::RealtimeNotEnabled)
        }

        #[cfg(feature = "realtime")]
        {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

            let host = cpal::default_host();
            let device = match &self.config.device_name {
                Some(name) => host
                    .output_devices()
                    .map_err(|error| HardwareError::StreamOpenFailed {
                        reason: error.to_string(),
                    })?
                    .find(|device| device.name().map(|value| value == *name).unwrap_or(false))
                    .ok_or_else(|| HardwareError::DeviceNotFound { name: name.clone() })?,
                None => host
                    .default_output_device()
                    .ok_or(HardwareError::NoDefaultDevice)?,
            };
            let device_name = device.name().unwrap_or_else(|_| "Unknown".into());
            let sample_rate = self.config.sample_rate as u32;
            let channels = self.config.n_channels as u16;
            let stream_config = cpal::StreamConfig {
                channels,
                sample_rate: cpal::SampleRate(sample_rate),
                buffer_size: cpal::BufferSize::Fixed(self.config.block_size as u32),
            };
            let mut callback = callback;
            let stream = device
                .build_output_stream(
                    &stream_config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| callback(data),
                    |error| eprintln!("Audio stream error: {error}"),
                    None,
                )
                .map_err(|error| HardwareError::StreamOpenFailed {
                    reason: error.to_string(),
                })?;
            stream
                .play()
                .map_err(|error| HardwareError::StreamOpenFailed {
                    reason: error.to_string(),
                })?;
            Ok(HardwareStream {
                _stream: stream,
                device_name,
                sample_rate,
                channels,
            })
        }
    }

    /// List available output device names.
    pub fn list_devices() -> Result<Vec<String>, HardwareError> {
        #[cfg(not(feature = "realtime"))]
        return Err(HardwareError::RealtimeNotEnabled);

        #[cfg(feature = "realtime")]
        {
            use cpal::traits::{DeviceTrait, HostTrait};
            let host = cpal::default_host();
            let devices = host
                .output_devices()
                .map_err(|e| HardwareError::StreamOpenFailed {
                    reason: e.to_string(),
                })?;
            let names: Vec<String> = devices.filter_map(|d| d.name().ok()).collect();
            Ok(names)
        }
    }
}

#[derive(Debug, Error)]
pub enum HardwareError {
    #[error("No audio device named '{name}'")]
    DeviceNotFound { name: String },
    #[error("No default audio output device found")]
    NoDefaultDevice,
    #[error("Failed to open audio stream: {reason}")]
    StreamOpenFailed { reason: String },
    #[error("Real-time feature not enabled. Rebuild with --features realtime")]
    RealtimeNotEnabled,
}
