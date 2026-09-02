use crate::{BackendKind, BatchProcessor, GpuError};
use cudarc::{
    driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg},
    nvrtc::{compile_ptx_with_opts, CompileOptions},
};
use std::sync::Arc;

const KERNEL: &str = r#"
extern "C" __global__ void schisma_condition(
    float *output,
    const float *input,
    int length,
    float gain,
    float drive
) {
    int index = blockIdx.x * blockDim.x + threadIdx.x;
    if (index < length) {
        output[index] = tanhf(input[index] * drive) * gain;
    }
}
"#;

pub struct CudaProcessor {
    _context: Arc<CudaContext>,
    stream: Arc<CudaStream>,
    function: CudaFunction,
    device_name: String,
}

impl CudaProcessor {
    pub fn new() -> Result<Self, GpuError> {
        if cfg!(target_os = "macos") {
            return Err(GpuError::Unavailable {
                backend: BackendKind::Cuda,
                reason: "NVIDIA does not provide a CUDA runtime for current macOS systems".into(),
            });
        }
        let context = std::panic::catch_unwind(|| CudaContext::new(0))
            .map_err(|_| GpuError::Unavailable {
                backend: BackendKind::Cuda,
                reason: "CUDA driver library could not be loaded".into(),
            })?
            .map_err(|error| GpuError::Unavailable {
                backend: BackendKind::Cuda,
                reason: error.to_string(),
            })?;
        let device_name = context.name().map_err(|error| GpuError::Unavailable {
            backend: BackendKind::Cuda,
            reason: error.to_string(),
        })?;
        let ptx = compile_ptx_with_opts(
            KERNEL,
            CompileOptions {
                ftz: Some(false),
                fmad: Some(true),
                ..Default::default()
            },
        )
        .map_err(|error| GpuError::Unavailable {
            backend: BackendKind::Cuda,
            reason: error.to_string(),
        })?;
        let module = context
            .load_module(ptx)
            .map_err(|error| GpuError::Unavailable {
                backend: BackendKind::Cuda,
                reason: error.to_string(),
            })?;
        let function =
            module
                .load_function("schisma_condition")
                .map_err(|error| GpuError::Unavailable {
                    backend: BackendKind::Cuda,
                    reason: error.to_string(),
                })?;
        let stream = context.default_stream();
        Ok(Self {
            _context: context,
            stream,
            function,
            device_name,
        })
    }
}

impl BatchProcessor for CudaProcessor {
    fn kind(&self) -> BackendKind {
        BackendKind::Cuda
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn condition(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        gain: f32,
        drive: f32,
    ) -> Result<(), GpuError> {
        if input.is_empty() {
            return Ok(());
        }
        let input_device = self.stream.clone_htod(input).map_err(cuda_compute_error)?;
        let mut output_device = self
            .stream
            .alloc_zeros::<f32>(input.len())
            .map_err(cuda_compute_error)?;
        let length = i32::try_from(input.len()).map_err(|error| GpuError::Compute {
            backend: BackendKind::Cuda,
            reason: error.to_string(),
        })?;
        let mut arguments = self.stream.launch_builder(&self.function);
        arguments.arg(&mut output_device);
        arguments.arg(&input_device);
        arguments.arg(&length);
        arguments.arg(&gain);
        arguments.arg(&drive);
        unsafe {
            arguments
                .launch(LaunchConfig::for_num_elems(input.len() as u32))
                .map_err(cuda_compute_error)?;
        }
        let result = self
            .stream
            .clone_dtoh(&output_device)
            .map_err(cuda_compute_error)?;
        output.copy_from_slice(&result);
        Ok(())
    }
}

fn cuda_compute_error(error: impl std::fmt::Display) -> GpuError {
    GpuError::Compute {
        backend: BackendKind::Cuda,
        reason: error.to_string(),
    }
}
