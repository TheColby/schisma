use crate::{BackendKind, BatchProcessor, GpuError};

pub struct CpuProcessor;

impl BatchProcessor for CpuProcessor {
    fn kind(&self) -> BackendKind {
        BackendKind::Cpu
    }

    fn device_name(&self) -> &str {
        "Scalar CPU fallback"
    }

    fn condition(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        gain: f32,
        drive: f32,
    ) -> Result<(), GpuError> {
        for (source, target) in input.iter().zip(output) {
            *target = (*source * drive).tanh() * gain;
        }
        Ok(())
    }
}
