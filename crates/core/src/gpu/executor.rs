//! The executor: the device's compute pipelines, one for each kernel of this directory, compiled the first time they are needed,
//! and the one way a kernel is run (`Executor::dispatch`). Nothing here knows what a scene is.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// The kernels, each a `.wgsl` file after the shared prelude.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Kernel {
    Warp,
    ReduceAcross,
    ReduceDown,
    Straight,
    Window,
    Blur,
    Plane,
    Planes,
    Join,
}

impl Kernel {
    fn wgsl(self) -> &'static str {
        match self {
            Self::Warp => include_str!("warp.wgsl"),
            Self::ReduceAcross => include_str!("reduce_across.wgsl"),
            Self::ReduceDown => include_str!("reduce_down.wgsl"),
            Self::Straight => include_str!("straight.wgsl"),
            Self::Window => include_str!("window.wgsl"),
            Self::Blur => include_str!("box.wgsl"),
            Self::Plane => include_str!("plane.wgsl"),
            Self::Planes => include_str!("planes.wgsl"),
            Self::Join => include_str!("join.wgsl"),
        }
    }
}

/// The WGSL every kernel is compiled with: the parameter block and its readers.
const PRELUDE: &str = include_str!("prelude.wgsl");

/// The side of a workgroup: 16 x 16 invocations, one a pixel.
const GROUP: u32 = 16; // constant: the `@workgroup_size(16, 16)` of every kernel

/// The integers and the floats a kernel is given, and the size of its output (the first two integers).
pub(super) struct Call {
    ints: [u32; 16],
    reals: [f32; 48],
}

impl Call {
    /// A call whose output is `columns` by `rows` invocations.
    pub(super) fn new(columns: usize, rows: usize) -> Self {
        let mut call = Self {
            ints: [0; 16],
            reals: [0.0; 48],
        };
        call.ints[0] = u32::try_from(columns).unwrap_or(u32::MAX);
        call.ints[1] = u32::try_from(rows).unwrap_or(u32::MAX);
        call
    }

    /// Integer `at` (2 and up) set.
    pub(super) fn int(mut self, at: usize, value: usize) -> Self {
        self.ints[at] = u32::try_from(value).unwrap_or(u32::MAX);
        self
    }

    /// A signed integer `at` set (it is read as an `i32` by the kernel).
    pub(super) fn signed(mut self, at: usize, value: i64) -> Self {
        self.ints[at] = i32::try_from(value).unwrap_or(i32::MAX).cast_unsigned();
        self
    }

    /// Float `at` set.
    pub(super) fn real(mut self, at: usize, value: f64) -> Self {
        #[expect(clippy::cast_possible_truncation, reason = "a shader works in 32-bit floats")]
        {
            self.reals[at] = value as f32;
        }
        self
    }

    fn bytes(&self) -> Vec<u8> {
        self.ints
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .chain(self.reals.iter().flat_map(|value| value.to_le_bytes()))
            .collect()
    }

    fn size(&self) -> (u32, u32) {
        (self.ints[0], self.ints[1])
    }
}

/// The storage buffers a kernel reads (`src`, `aux`) and writes (`dst`).
pub(super) struct Bindings<'a> {
    pub(super) src: Option<&'a wgpu::Buffer>,
    pub(super) dst: &'a wgpu::Buffer,
    pub(super) aux: Option<&'a wgpu::Buffer>,
}

pub(super) struct Executor {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: Mutex<HashMap<Kernel, wgpu::ComputePipeline>>,
    /// What a kernel's unused storage bindings hold.
    empty: wgpu::Buffer,
}

/// The storage buffers a kernel binds: the device must allow as many to a stage.
const STORAGE_BINDINGS: u32 = 3; // constant: the binding layout below

impl Executor {
    /// The executor on `device`, or the name of the limit the device lacks.
    pub(super) fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
        let limits = device.limits();
        for (name, has, needs) in [
            (
                "max_storage_buffers_per_shader_stage",
                limits.max_storage_buffers_per_shader_stage,
                STORAGE_BINDINGS,
            ),
            (
                "max_compute_invocations_per_workgroup",
                limits.max_compute_invocations_per_workgroup,
                GROUP * GROUP,
            ),
        ] {
            if has < needs {
                return Err(format!("the device's `{name}` is {has}; the GPU compositor needs at least {needs}"));
            }
        }
        let entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let read = wgpu::BufferBindingType::Storage { read_only: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bullterrier-scene kernel"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, read),
                entry(2, wgpu::BufferBindingType::Storage { read_only: false }),
                entry(3, read),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bullterrier-scene kernel"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let empty = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bullterrier-scene unused binding"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Ok(Self {
            device,
            queue,
            layout,
            pipeline_layout,
            pipelines: Mutex::new(HashMap::new()),
            empty,
        })
    }

    /// The largest buffer a kernel can bind, in bytes.
    pub(super) fn largest_binding(&self) -> u64 {
        let limits = self.device.limits();
        limits.max_storage_buffer_binding_size.min(limits.max_buffer_size)
    }

    /// The longest side a dispatch covers.
    pub(super) fn longest_side(&self) -> usize {
        self.device.limits().max_compute_workgroups_per_dimension as usize * GROUP as usize
    }

    /// A zeroed storage buffer of `bytes` bytes (at least 16) that kernels read and write and the queue copies.
    pub(super) fn buffer(&self, bytes: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bullterrier-scene buffer"),
            size: bytes.max(16),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// A buffer holding `values`.
    pub(super) fn sent(&self, values: &[f32]) -> wgpu::Buffer {
        let bytes: Vec<u8> = values.iter().flat_map(|value| value.to_le_bytes()).collect();
        let buffer = self.buffer(bytes.len() as u64);
        self.queue.write_buffer(&buffer, 0, &bytes);
        buffer
    }

    pub(super) fn encoder(&self) -> wgpu::CommandEncoder {
        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bullterrier-scene"),
        })
    }

    fn pipeline(&self, kernel: Kernel) -> wgpu::ComputePipeline {
        let mut pipelines = self.pipelines.lock().unwrap_or_else(PoisonError::into_inner);
        pipelines
            .entry(kernel)
            .or_insert_with(|| {
                let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&format!("{kernel:?}")),
                    source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}\n{}", kernel.wgsl()).into()),
                });
                self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(&format!("{kernel:?}")),
                    layout: Some(&self.pipeline_layout),
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                })
            })
            .clone()
    }

    /// Records into `encoder` one run of `kernel` over the output size `call` names, one invocation a pixel.
    pub(super) fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        kernel: Kernel,
        call: &Call,
        bindings: &Bindings<'_>,
    ) {
        let pipeline = self.pipeline(kernel);
        let uniform = call.bytes();
        let settings = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bullterrier-scene parameters"),
            size: uniform.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&settings, 0, &uniform);
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bullterrier-scene kernel"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: settings.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bindings.src.unwrap_or(&self.empty).as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: bindings.dst.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: bindings.aux.unwrap_or(&self.empty).as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&format!("{kernel:?}")),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        let (columns, rows) = call.size();
        pass.dispatch_workgroups(columns.div_ceil(GROUP), rows.div_ceil(GROUP), 1);
    }
}
