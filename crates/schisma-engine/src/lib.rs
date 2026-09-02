//! Internal M0 prototype for the controller-first per-note instrument.
//!
//! This crate deliberately contains a fixed signal chain. Its job is to prove
//! MPE voice semantics, mapped tuning, the wavetable-to-modal-body morph, and
//! realtime behavior before a graph editor or application shell is built.

pub mod dsp;
pub mod engine;
pub mod mpe;
pub mod rt_audit;
pub mod telemetry;

pub use engine::{
    default_twelve_tet_tuning, M0Config, M0Engine, MAX_SUPPORTED_SAMPLE_RATE_HZ,
    MIN_SUPPORTED_SAMPLE_RATE_HZ, OUTPUT_CHANNELS,
};
pub use mpe::{MpeVoiceManager, VoicePhase, VoiceState, ZoneConfig};

#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: rt_audit::AuditAllocator = rt_audit::AuditAllocator;
