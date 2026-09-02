//! Host-facing audio device adapters for Schisma.
//!
//! The core engine does not own an audio callback or window. This crate keeps
//! standalone device management outside the synthesis contract.

pub mod hardware;

pub use hardware::{HardwareConfig, HardwareError, HardwareHost};
