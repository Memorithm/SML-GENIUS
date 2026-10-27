//! Vendor-neutral GPU twin of the PTX kernels (feature `portable-gpu`).
//!
//! WGSL is compiled by `naga` (inside `wgpu`) to SPIR-V, Metal or DXIL and run
//! through the system driver (Vulkan, Metal, DX12). No vendor SDK is involved.
//! See `docs/SML-GROK-NATIVE-A2-PORTABLE-GPU.md`.

use crate::PtxOperation;
use wgpu::util::DeviceExt;

/// Verbatim WGSL source; entry points share the PTX symbol names.
pub const WGSL: &str = include_str!("../wgsl/boolean_words.wgsl");

/// Portable GPU failure; never replaced by a simulated result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortableGpuError {
    /// No adapter/driver was found.
    Unavailable(String),
    /// Inputs do not share one length.
    LengthMismatch,
    /// Device or readback failure.
    Device(String),
}

impl std::fmt::Display for PortableGpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PortableGpuError {}

/// Executed adapter identity, so evidence names what actually ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterIdentity {
    /// Driver-reported adapter name.
    pub name: String,
    /// Backend (Vulkan, Metal, Dx12, Gl).
    pub backend: String,
    /// `SOFTWARE_RASTERIZER` for CPU adapters (e.g. lavapipe), else device type.
    pub kind: String,
}

/// A ready device + compiled module.
pub struct PortableGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    bind_layout: wgpu::BindGroupLayout,
    identity: AdapterIdentity,
}

impl PortableGpu {
    /// Open the default adapter, or report `Unavailable`.
    pub fn open() -> Result<Self, PortableGpuError> {
        pollster::block_on(Self::open_async())
    }

    async fn open_async() -> Result<Self, PortableGpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| PortableGpuError::Unavailable(e.to_string()))?;
        let info = adapter.get_info();
        let kind = if info.device_type == wgpu::DeviceType::Cpu {
            "SOFTWARE_RASTERIZER".to_string()
        } else {
            format!("{:?}", info.device_type)
        };
        let identity = AdapterIdentity {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            kind,
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| PortableGpuError::Device(e.to_string()))?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sml_boolean_words"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        // Explicit layout: auto-derived layouts drop bindings an entry point
        // does not read (Hamming ignores `c`), which fails validation.
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sml_boolean_words"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, true),
                storage(3, false),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sml_boolean_words"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        Ok(Self {
            layout,
            bind_layout,
            device,
            queue,
            module,
            identity,
        })
    }

    /// What actually executed.
    #[must_use]
    pub fn identity(&self) -> &AdapterIdentity {
        &self.identity
    }

    /// Run `operation` over matching-length lanes; one u32 per lane.
    pub fn execute(
        &self,
        operation: PtxOperation,
        a: &[u32],
        b: &[u32],
        c: &[u32],
    ) -> Result<Vec<u32>, PortableGpuError> {
        if a.len() != b.len() || a.len() != c.len() {
            return Err(PortableGpuError::LengthMismatch);
        }
        if a.is_empty() {
            return Ok(Vec::new());
        }
        let d = &self.device;
        let input = |label, data: &[u32]| {
            d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: &words_to_bytes(data),
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let (ba, bb, bc) = (input("a", a), input("b", b), input("c", c));
        let size = (a.len() * 4) as u64;
        let out = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("o"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pipeline = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(operation.symbol()),
            layout: Some(&self.layout),
            module: &self.module,
            entry_point: Some(operation.symbol()),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_layout,
            entries: &[entry(0, &ba), entry(1, &bb), entry(2, &bc), entry(3, &out)],
        });
        let groups = u32::try_from(a.len().div_ceil(64))
            .map_err(|_| PortableGpuError::Device("too many lanes".into()))?;
        let mut enc = d.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(groups, 1, 1);
        }
        enc.copy_buffer_to_buffer(&out, 0, &read, 0, size);
        self.queue.submit([enc.finish()]);
        let slice = read.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        d.poll(wgpu::PollType::Wait)
            .map_err(|e| PortableGpuError::Device(e.to_string()))?;
        rx.recv()
            .map_err(|e| PortableGpuError::Device(e.to_string()))?
            .map_err(|e| PortableGpuError::Device(e.to_string()))?;
        let bytes = slice.get_mapped_range();
        let result = bytes
            .chunks_exact(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        drop(bytes);
        read.unmap();
        Ok(result)
    }
}

fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn words_to_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ptx_word_oracle, CpuKernels};

    struct SplitMix64(u64);
    impl SplitMix64 {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
    }

    fn lanes() -> (Vec<u32>, Vec<u32>, Vec<u32>) {
        let mut edge = vec![0, u32::MAX, 0xAAAA_AAAA, 0x5555_5555];
        edge.extend((0..32).map(|i| 1u32 << i));
        let mut r = SplitMix64(0xA2);
        let n = 65_537;
        let mut v = |k: usize| -> Vec<u32> {
            let mut out: Vec<u32> = edge.iter().map(|e| e.rotate_left(k as u32)).collect();
            while out.len() < n {
                out.push(r.next_u64() as u32);
            }
            out
        };
        (v(0), v(7), v(13))
    }

    #[test]
    fn cpu_asm_matches_portable_rust_on_same_words() {
        let (a, b, c) = lanes();
        let (asm, rust) = (CpuKernels::detect(), CpuKernels::portable());
        let pack = |v: &[u32], i: usize| u64::from(v[i]) | (u64::from(v[i + 1]) << 32);
        for i in (0..a.len() - 1).step_by(2) {
            let (x, y, z) = (pack(&a, i), pack(&b, i), pack(&c, i));
            assert_eq!(asm.hamming(x, y), rust.hamming(x, y));
            assert_eq!(asm.gf2_dot_parity(x, y), rust.gf2_dot_parity(x, y));
            assert_eq!(asm.ternary_partial(x, y, z), rust.ternary_partial(x, y, z));
        }
    }

    #[test]
    fn portable_gpu_is_bit_exact_with_oracle() {
        let gpu = match PortableGpu::open() {
            Ok(g) => g,
            Err(e) => {
                assert!(
                    std::env::var_os("SML_PORTABLE_GPU_REQUIRED").is_none(),
                    "adapter required but unavailable: {e}"
                );
                eprintln!("SKIPPED portable GPU: {e}");
                return;
            }
        };
        eprintln!("portable GPU adapter: {:?}", gpu.identity());
        let (a, b, c) = lanes();
        for op in [
            PtxOperation::Xor3,
            PtxOperation::Hamming,
            PtxOperation::TernaryPartial,
        ] {
            let got = gpu.execute(op, &a, &b, &c).expect("execute");
            assert_eq!(got.len(), a.len());
            for i in 0..a.len() {
                assert_eq!(
                    got[i],
                    ptx_word_oracle(op, a[i], b[i], c[i]),
                    "{op:?} lane {i}"
                );
            }
        }
        assert_eq!(
            gpu.execute(PtxOperation::Xor3, &a, &b[1..], &c),
            Err(PortableGpuError::LengthMismatch)
        );
    }
}
