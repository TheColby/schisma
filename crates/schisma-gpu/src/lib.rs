//! GPU-accelerated batch signal conditioning for Schisma analysis and offline DSP.
//!
//! GPU work never runs synchronously inside the audio callback. Metal and CUDA
//! backends share the same deterministic `f32` operation and use CPU fallback
//! when the requested runtime is unavailable.

mod cpu;
#[cfg(feature = "cuda")]
mod cuda;
#[cfg(feature = "metal")]
mod metal;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Auto,
    Cpu,
    Metal,
    Cuda,
}

impl BackendKind {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Cpu, Self::Metal, Self::Cuda];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Cpu => "CPU",
            Self::Metal => "Metal",
            Self::Cuda => "CUDA",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendStatus {
    pub kind: BackendKind,
    pub available: bool,
    pub device_name: String,
    pub detail: String,
}

pub trait BatchProcessor: Send {
    fn kind(&self) -> BackendKind;
    fn device_name(&self) -> &str;
    fn condition(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        gain: f32,
        drive: f32,
    ) -> Result<(), GpuError>;
}

pub struct Accelerator {
    requested: BackendKind,
    processor: Box<dyn BatchProcessor>,
    fallback_reason: Option<String>,
}

impl Accelerator {
    pub fn new(requested: BackendKind) -> Self {
        let candidates: &[BackendKind] = match requested {
            BackendKind::Auto if cfg!(target_os = "macos") => {
                &[BackendKind::Metal, BackendKind::Cuda, BackendKind::Cpu]
            }
            BackendKind::Auto => &[BackendKind::Cuda, BackendKind::Metal, BackendKind::Cpu],
            BackendKind::Cpu => &[BackendKind::Cpu],
            BackendKind::Metal => &[BackendKind::Metal, BackendKind::Cpu],
            BackendKind::Cuda => &[BackendKind::Cuda, BackendKind::Cpu],
        };

        let mut failures = Vec::new();
        for kind in candidates {
            match create_processor(*kind) {
                Ok(processor) => {
                    return Self {
                        requested,
                        fallback_reason: (!failures.is_empty()).then(|| failures.join("; ")),
                        processor,
                    };
                }
                Err(error) => failures.push(error.to_string()),
            }
        }
        unreachable!("CPU processor construction is infallible")
    }

    pub fn requested(&self) -> BackendKind {
        self.requested
    }

    pub fn active(&self) -> BackendKind {
        self.processor.kind()
    }

    pub fn device_name(&self) -> &str {
        self.processor.device_name()
    }

    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }

    pub fn condition(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        gain: f32,
        drive: f32,
    ) -> Result<(), GpuError> {
        if input.len() != output.len() {
            return Err(GpuError::LengthMismatch {
                input: input.len(),
                output: output.len(),
            });
        }
        self.processor.condition(input, output, gain, drive)
    }

    pub fn self_test(&mut self) -> Result<(), GpuError> {
        let input = [-0.75, -0.25, 0.0, 0.25, 0.75];
        let mut output = [0.0; 5];
        self.condition(&input, &mut output, 0.8, 1.4)?;
        for (actual, sample) in output.iter().zip(input) {
            let expected = (sample * 1.4_f32).tanh() * 0.8;
            if (*actual - expected).abs() > 2.0e-5 {
                return Err(GpuError::SelfTest {
                    backend: self.active(),
                    expected,
                    actual: *actual,
                });
            }
        }
        Ok(())
    }
}

pub fn discover() -> Vec<BackendStatus> {
    [BackendKind::Metal, BackendKind::Cuda, BackendKind::Cpu]
        .into_iter()
        .map(|kind| match create_processor(kind) {
            Ok(processor) => BackendStatus {
                kind,
                available: true,
                device_name: processor.device_name().into(),
                detail: "ready".into(),
            },
            Err(error) => BackendStatus {
                kind,
                available: false,
                device_name: "Unavailable".into(),
                detail: error.to_string(),
            },
        })
        .collect()
}

fn create_processor(kind: BackendKind) -> Result<Box<dyn BatchProcessor>, GpuError> {
    match kind {
        BackendKind::Auto => Err(GpuError::Unavailable {
            backend: kind,
            reason: "Auto is a selection policy, not a compute API".into(),
        }),
        BackendKind::Cpu => Ok(Box::new(cpu::CpuProcessor)),
        BackendKind::Metal => {
            #[cfg(feature = "metal")]
            {
                Ok(Box::new(metal::MetalProcessor::new()?))
            }
            #[cfg(not(feature = "metal"))]
            {
                Err(GpuError::Unavailable {
                    backend: kind,
                    reason: "Schisma was built without the metal feature".into(),
                })
            }
        }
        BackendKind::Cuda => {
            #[cfg(feature = "cuda")]
            {
                Ok(Box::new(cuda::CudaProcessor::new()?))
            }
            #[cfg(not(feature = "cuda"))]
            {
                Err(GpuError::Unavailable {
                    backend: kind,
                    reason: "Schisma was built without the cuda feature".into(),
                })
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("{backend:?} unavailable: {reason}")]
    Unavailable {
        backend: BackendKind,
        reason: String,
    },
    #[error("GPU input length {input} does not match output length {output}")]
    LengthMismatch { input: usize, output: usize },
    #[error("{backend:?} compute failed: {reason}")]
    Compute {
        backend: BackendKind,
        reason: String,
    },
    #[error("{backend:?} self-test expected {expected}, received {actual}")]
    SelfTest {
        backend: BackendKind,
        expected: f32,
        actual: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_backend_matches_reference() {
        let mut accelerator = Accelerator::new(BackendKind::Cpu);
        accelerator.self_test().unwrap();
    }

    #[test]
    fn mismatched_buffers_are_rejected() {
        let mut accelerator = Accelerator::new(BackendKind::Cpu);
        assert!(matches!(
            accelerator.condition(&[1.0], &mut [], 1.0, 1.0),
            Err(GpuError::LengthMismatch { .. })
        ));
    }

    #[cfg(all(target_os = "macos", feature = "metal"))]
    #[test]
    fn metal_backend_executes_the_reference_kernel() {
        let mut accelerator = Accelerator::new(BackendKind::Metal);
        assert_eq!(accelerator.active(), BackendKind::Metal);
        accelerator.self_test().unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cuda_request_falls_back_cleanly_on_macos() {
        let accelerator = Accelerator::new(BackendKind::Cuda);
        assert_eq!(accelerator.active(), BackendKind::Cpu);
        assert!(accelerator.fallback_reason().is_some());
    }
}
