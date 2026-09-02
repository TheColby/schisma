use crate::{BackendKind, BatchProcessor, GpuError};
use bytemuck::{Pod, Zeroable};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct Params {
    gain: f32,
    drive: f32,
    length: u32,
    _padding: u32,
}

@group(0) @binding(0) var<storage, read> input_samples: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_samples: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(256)
fn condition(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index < params.length) {
        output_samples[index] = tanh(input_samples[index] * params.drive) * params.gain;
    }
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    gain: f32,
    drive: f32,
    length: u32,
    padding: u32,
}

pub struct MetalProcessor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    device_name: String,
}

impl MetalProcessor {
    pub fn new() -> Result<Self, GpuError> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::METAL;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|error| GpuError::Unavailable {
            backend: BackendKind::Metal,
            reason: error.to_string(),
        })?;
        let info = adapter.get_info();
        if info.backend != wgpu::Backend::Metal {
            return Err(GpuError::Unavailable {
                backend: BackendKind::Metal,
                reason: format!("adapter '{}' is {:?}, not Metal", info.name, info.backend),
            });
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Schisma Metal compute device"),
            ..Default::default()
        }))
        .map_err(|error| GpuError::Unavailable {
            backend: BackendKind::Metal,
            reason: error.to_string(),
        })?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Schisma signal conditioning"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Schisma Metal conditioning pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("condition"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            device_name: info.name,
        })
    }
}

impl BatchProcessor for MetalProcessor {
    fn kind(&self) -> BackendKind {
        BackendKind::Metal
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
        let byte_size = std::mem::size_of_val(input) as u64;
        let input_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Schisma Metal input"),
                contents: bytemuck::cast_slice(input),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Schisma Metal output"),
            size: byte_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let params = Params {
            gain,
            drive,
            length: input.len() as u32,
            padding: 0,
        };
        let params_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Schisma Metal parameters"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Schisma Metal readback"),
            size: byte_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let layout = self.pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Schisma Metal conditioning bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Schisma Metal conditioning encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Schisma Metal conditioning pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((input.len() as u32).div_ceil(256), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging, 0, byte_size);
        self.queue.submit(Some(encoder.finish()));

        let slice = staging.slice(..);
        let (sender, receiver) = mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| GpuError::Compute {
                backend: BackendKind::Metal,
                reason: error.to_string(),
            })?;
        receiver
            .recv()
            .map_err(|error| GpuError::Compute {
                backend: BackendKind::Metal,
                reason: error.to_string(),
            })?
            .map_err(|error| GpuError::Compute {
                backend: BackendKind::Metal,
                reason: error.to_string(),
            })?;
        let bytes = slice.get_mapped_range();
        output.copy_from_slice(bytemuck::cast_slice(&bytes));
        drop(bytes);
        staging.unmap();
        Ok(())
    }
}
