//! Optional CUDA Driver API PTX launcher. Driver-only: no CUDA Toolkit,
//! no libcudart and no cuBLAS at build time. A real NVIDIA GPU is required
//! for device execution and benchmarking. This is not enabled by default.

use crate::{PtxOperation, NVIDIA_PTX};
use core::ffi::{c_char, c_int, c_void};
use std::{ffi::CString, mem, ptr};

#[link(name = "dl")]
extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}
type Context = *mut c_void;
type Module = *mut c_void;
type Function = *mut c_void;
type DevicePtr = u64;
type FnInit = unsafe extern "C" fn(u32) -> i32;
type FnDevice = unsafe extern "C" fn(*mut i32, i32) -> i32;
type FnContextCreate = unsafe extern "C" fn(*mut Context, u32, i32) -> i32;
type FnContextDestroy = unsafe extern "C" fn(Context) -> i32;
type FnModuleLoad = unsafe extern "C" fn(*mut Module, *const c_void) -> i32;
type FnModuleUnload = unsafe extern "C" fn(Module) -> i32;
type FnGetFunction = unsafe extern "C" fn(*mut Function, Module, *const c_char) -> i32;
type FnAlloc = unsafe extern "C" fn(*mut DevicePtr, usize) -> i32;
type FnFree = unsafe extern "C" fn(DevicePtr) -> i32;
type FnH2D = unsafe extern "C" fn(DevicePtr, *const c_void, usize) -> i32;
type FnD2H = unsafe extern "C" fn(*mut c_void, DevicePtr, usize) -> i32;
type FnSync = unsafe extern "C" fn() -> i32;
type Event = *mut c_void;
type FnEventCreate = unsafe extern "C" fn(*mut Event, u32) -> i32;
type FnEventRecord = unsafe extern "C" fn(Event, *mut c_void) -> i32;
type FnEventSync = unsafe extern "C" fn(Event) -> i32;
type FnEventElapsed = unsafe extern "C" fn(*mut f32, Event, Event) -> i32;
type FnEventDestroy = unsafe extern "C" fn(Event) -> i32;
type FnLaunch = unsafe extern "C" fn(
    Function,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
    *mut c_void,
    *mut *mut c_void,
    *mut *mut c_void,
) -> i32;

/// An explicit GPU request fails closed if the driver or input contract fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuError {
    /// Driver library, function or PTX module was unavailable.
    Unavailable(&'static str),
    /// Native CUDA Driver API returned a nonzero error code.
    Driver(&'static str, i32),
    /// Input vectors do not have identical element counts.
    Shape(usize, usize, usize),
    /// Avoid excessively large experimental memory allocations.
    Oversize(usize),
}

fn check(name: &'static str, result: i32) -> Result<(), GpuError> {
    if result == 0 {
        Ok(())
    } else {
        Err(GpuError::Driver(name, result))
    }
}

/// # Safety
/// Caller guarantees the C ABI type corresponds to this fixed CUDA symbol,
/// and keeps the library loaded until all uses of the returned pointer end.
unsafe fn load<T: Copy>(
    library: *mut c_void,
    name: &std::ffi::CStr,
    label: &'static str,
) -> Result<T, GpuError> {
    // SAFETY: dlopen handle and NUL-terminated constant are valid.
    let pointer = unsafe { dlsym(library, name.as_ptr()) };
    if pointer.is_null() || mem::size_of::<T>() != mem::size_of::<*mut c_void>() {
        return Err(GpuError::Unavailable(label));
    }
    // SAFETY: exact CUDA C function signature was frozen in the caller.
    Ok(unsafe { mem::transmute_copy::<*mut c_void, T>(&pointer) })
}

struct Driver {
    library: *mut c_void,
    init: FnInit,
    device: FnDevice,
    ctx_create: FnContextCreate,
    ctx_destroy: FnContextDestroy,
    module_load: FnModuleLoad,
    module_unload: FnModuleUnload,
    function: FnGetFunction,
    alloc: FnAlloc,
    free: FnFree,
    h2d: FnH2D,
    d2h: FnD2H,
    launch: FnLaunch,
    sync: FnSync,
    event_create: FnEventCreate,
    event_record: FnEventRecord,
    event_sync: FnEventSync,
    event_elapsed: FnEventElapsed,
    event_destroy: FnEventDestroy,
}

impl Driver {
    fn new() -> Result<Self, GpuError> {
        // SAFETY: fixed trusted driver soname, checked for NULL below.
        let library = unsafe { dlopen(c"libcuda.so.1".as_ptr(), 2) };
        if library.is_null() {
            return Err(GpuError::Unavailable("libcuda.so.1"));
        }
        let result = (|| {
            // SAFETY: all names are fixed CUDA Driver C ABI symbols.
            unsafe {
                Ok(Self {
                    library,
                    init: load(library, c"cuInit", "cuInit")?,
                    device: load(library, c"cuDeviceGet", "cuDeviceGet")?,
                    ctx_create: load(library, c"cuCtxCreate_v2", "cuCtxCreate_v2")?,
                    ctx_destroy: load(library, c"cuCtxDestroy_v2", "cuCtxDestroy_v2")?,
                    module_load: load(library, c"cuModuleLoadData", "cuModuleLoadData")?,
                    module_unload: load(library, c"cuModuleUnload", "cuModuleUnload")?,
                    function: load(library, c"cuModuleGetFunction", "cuModuleGetFunction")?,
                    alloc: load(library, c"cuMemAlloc_v2", "cuMemAlloc_v2")?,
                    free: load(library, c"cuMemFree_v2", "cuMemFree_v2")?,
                    h2d: load(library, c"cuMemcpyHtoD_v2", "cuMemcpyHtoD_v2")?,
                    d2h: load(library, c"cuMemcpyDtoH_v2", "cuMemcpyDtoH_v2")?,
                    launch: load(library, c"cuLaunchKernel", "cuLaunchKernel")?,
                    sync: load(library, c"cuCtxSynchronize", "cuCtxSynchronize")?,
                    event_create: load(library, c"cuEventCreate", "cuEventCreate")?,
                    event_record: load(library, c"cuEventRecord", "cuEventRecord")?,
                    event_sync: load(library, c"cuEventSynchronize", "cuEventSynchronize")?,
                    event_elapsed: load(library, c"cuEventElapsedTime", "cuEventElapsedTime")?,
                    event_destroy: load(library, c"cuEventDestroy_v2", "cuEventDestroy_v2")?,
                })
            }
        })();
        if result.is_err() {
            // SAFETY: library belongs to this loader on failed resolution.
            unsafe { dlclose(library) };
        }
        result
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        // SAFETY: owned, non-null dlopen handle; no CUDA resources survive.
        unsafe { dlclose(self.library) };
    }
}

struct Session {
    driver: Driver,
    context: Context,
    module: Module,
    buffers: Vec<DevicePtr>,
}

impl Session {
    fn new() -> Result<Self, GpuError> {
        let mut session = Self {
            driver: Driver::new()?,
            context: ptr::null_mut(),
            module: ptr::null_mut(),
            buffers: Vec::new(),
        };
        // SAFETY: all output pointers are live owned fields or locals.
        unsafe {
            check("cuInit", (session.driver.init)(0))?;
            let mut device = 0_i32;
            check("cuDeviceGet", (session.driver.device)(&mut device, 0))?;
            check(
                "cuCtxCreate_v2",
                (session.driver.ctx_create)(&mut session.context, 0, device),
            )?;
            let ptx = CString::new(NVIDIA_PTX).map_err(|_| GpuError::Unavailable("PTX NUL"))?;
            check(
                "cuModuleLoadData",
                (session.driver.module_load)(&mut session.module, ptx.as_ptr().cast()),
            )?;
        }
        Ok(session)
    }

    fn allocate(&mut self, source: Option<&[u32]>, words: usize) -> Result<DevicePtr, GpuError> {
        let bytes = words * mem::size_of::<u32>();
        let mut device = 0_u64;
        // SAFETY: bounded count and initialized, owned device allocation.
        unsafe { check("cuMemAlloc_v2", (self.driver.alloc)(&mut device, bytes))? };
        self.buffers.push(device);
        if let Some(host) = source {
            // SAFETY: host slice has words elements; allocation has bytes.
            unsafe {
                check(
                    "cuMemcpyHtoD_v2",
                    (self.driver.h2d)(device, host.as_ptr().cast(), bytes),
                )?;
            }
        }
        Ok(device)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: all device buffers, module and context are uniquely owned
        // and freed before libcuda is closed by the Driver field destructor.
        unsafe {
            for device in self.buffers.drain(..) {
                (self.driver.free)(device);
            }
            if !self.module.is_null() {
                (self.driver.module_unload)(self.module);
            }
            if !self.context.is_null() {
                (self.driver.ctx_destroy)(self.context);
            }
        }
    }
}

/// Run a GPU bit-word operation with synchronous host/device transfers.
/// The caller can separately benchmark PCIe/UMA transfers and kernel time.
/// No automatic inference dispatch is made.
///
/// # Errors
/// Returns shape, size, symbol, JIT or CUDA driver errors; never a fake GPU
/// result. Signed ternary partials are encoded as two's-complement u32.
pub fn execute(
    operation: PtxOperation,
    a: &[u32],
    b: &[u32],
    c: &[u32],
) -> Result<Vec<u32>, GpuError> {
    if a.len() != b.len() || a.len() != c.len() {
        return Err(GpuError::Shape(a.len(), b.len(), c.len()));
    }
    if a.len() > (1 << 24) {
        return Err(GpuError::Oversize(a.len()));
    }
    if a.is_empty() {
        return Ok(Vec::new());
    }

    let mut session = Session::new()?;
    let mut input_a = session.allocate(Some(a), a.len())?;
    let mut input_b = session.allocate(Some(b), b.len())?;
    let mut input_c = session.allocate(Some(c), c.len())?;
    let mut output = session.allocate(None, a.len())?;
    let mut length = a.len() as u32;
    let mut function: Function = ptr::null_mut();
    let symbol =
        CString::new(operation.symbol()).map_err(|_| GpuError::Unavailable("symbol NUL"))?;
    // SAFETY: live module/function; each pointer points to a live CUDA
    // kernel parameter for the full duration of the synchronous launch.
    unsafe {
        check(
            "cuModuleGetFunction",
            (session.driver.function)(&mut function, session.module, symbol.as_ptr()),
        )?;
        let mut parameters = [
            (&mut input_a as *mut DevicePtr).cast::<c_void>(),
            (&mut input_b as *mut DevicePtr).cast::<c_void>(),
            (&mut input_c as *mut DevicePtr).cast::<c_void>(),
            (&mut output as *mut DevicePtr).cast::<c_void>(),
            (&mut length as *mut u32).cast::<c_void>(),
        ];
        check(
            "cuLaunchKernel",
            (session.driver.launch)(
                function,
                length.div_ceil(256),
                1,
                1,
                256,
                1,
                1,
                0,
                ptr::null_mut(),
                parameters.as_mut_ptr(),
                ptr::null_mut(),
            ),
        )?;
        check("cuCtxSynchronize", (session.driver.sync)())?;
    }
    let mut result = vec![0_u32; a.len()];
    // SAFETY: GPU kernel has completed, destination is an initialized,
    // exclusively owned Vec of a.len() elements and allocation matches size.
    unsafe {
        check(
            "cuMemcpyDtoH_v2",
            (session.driver.d2h)(
                result.as_mut_ptr().cast(),
                output,
                mem::size_of_val(result.as_slice()),
            ),
        )?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ptx_word_oracle;

    #[test]
    fn rejects_invalid_lengths_without_cuda_or_allocation() {
        assert_eq!(
            execute(PtxOperation::Hamming, &[1, 2], &[0], &[1, 2]),
            Err(GpuError::Shape(2, 1, 2))
        );
        assert_eq!(execute(PtxOperation::Xor3, &[], &[], &[]), Ok(Vec::new()));
    }

    #[test]
    fn exact_real_gpu_vectors_only_when_requested() {
        if std::env::var_os("SML_NATIVE_GPU_TEST").is_none() {
            return;
        }
        let n = 1031;
        let a: Vec<u32> = (0..n)
            .map(|x| (x as u32).wrapping_mul(0x9e37_79b9))
            .collect();
        let b: Vec<u32> = (0..n).map(|x| !(x as u32).rotate_left(7)).collect();
        let c: Vec<u32> = (0..n).map(|x| (x as u32).rotate_right(13)).collect();
        for op in [
            PtxOperation::Xor3,
            PtxOperation::Hamming,
            PtxOperation::TernaryPartial,
        ] {
            let observed = execute(op, &a, &b, &c).expect("GPU required by SML_NATIVE_GPU_TEST");
            let expected: Vec<_> = a
                .iter()
                .zip(&b)
                .zip(&c)
                .map(|((&x, &y), &z)| ptx_word_oracle(op, x, y, z))
                .collect();
            assert_eq!(observed, expected, "operation={op:?}");
        }
    }
}

/// Driver-managed CUDA events with lifetime bounded by the owning session.
/// Drop cleans up both events even if any measurement operation fails.
struct TimedEvents<'a> {
    driver: &'a Driver,
    first: Event,
    last: Event,
}

impl<'a> TimedEvents<'a> {
    fn new(driver: &'a Driver) -> Result<Self, GpuError> {
        let mut events = Self {
            driver,
            first: ptr::null_mut(),
            last: ptr::null_mut(),
        };
        // SAFETY: CUDA current context belongs to the live calling thread,
        // and the two event output pointers are valid owned fields.
        unsafe {
            check(
                "cuEventCreate:first",
                (events.driver.event_create)(&mut events.first, 0),
            )?;
            check(
                "cuEventCreate:last",
                (events.driver.event_create)(&mut events.last, 0),
            )?;
        }
        Ok(events)
    }

    fn mark(&self, event: Event) -> Result<(), GpuError> {
        // SAFETY: both events were initialized within this live context.
        // NULL stream means the legacy default CUDA stream.
        unsafe {
            check(
                "cuEventRecord",
                (self.driver.event_record)(event, ptr::null_mut()),
            )
        }
    }

    fn elapsed_ms(&self) -> Result<f32, GpuError> {
        let mut elapsed = 0.0_f32;
        // SAFETY: both recorded events belong to the current CUDA context;
        // stop-event synchronization completes all earlier default-stream work.
        unsafe {
            check("cuEventSynchronize", (self.driver.event_sync)(self.last))?;
            check(
                "cuEventElapsedTime",
                (self.driver.event_elapsed)(&mut elapsed, self.first, self.last),
            )?;
        }
        Ok(elapsed)
    }
}

impl Drop for TimedEvents<'_> {
    fn drop(&mut self) {
        // SAFETY: event handles are owned by this struct and never reused
        // after release. NULL means event creation previously failed.
        unsafe {
            if !self.last.is_null() {
                (self.driver.event_destroy)(self.last);
            }
            if !self.first.is_null() {
                (self.driver.event_destroy)(self.first);
            }
        }
    }
}

/// Reuses one CUDA context, one compiled PTX module and four device buffers
/// across many operations. All methods must be called on the creating thread.
///
/// Device data is explicitly uploaded, never silently regenerated. No resident
/// state persists after Drop. The input triplet is immutable between uploads.
pub struct ResidentGpu {
    session: Session,
    a: DevicePtr,
    b: DevicePtr,
    c: DevicePtr,
    out: DevicePtr,
    capacity_words: usize,
    kernels: [Function; 3],
}

/// Device elapsed time averaged over repeated warmed-up launches in the
/// default CUDA stream. Does not include host/device copy, JIT or allocations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuKernelTiming {
    /// Number of launches included after one warmup launch.
    pub iterations: u32,
    /// CUDA event elapsed time, milliseconds for all measured launches.
    pub total_device_ms: f32,
    /// CUDA event elapsed time per launch, in milliseconds.
    pub device_ms_per_launch: f32,
}

impl ResidentGpu {
    /// Creates bounded device allocations and resolves three PTX symbols once.
    /// A zero-capacity device is never instantiated.
    ///
    /// # Errors
    /// Returns a hardware/driver error or an allocation-policy error. No
    /// emulated result is ever substituted when real device work fails.
    pub fn new(capacity_words: usize) -> Result<Self, GpuError> {
        if capacity_words == 0 || capacity_words > (1 << 24) {
            return Err(GpuError::Oversize(capacity_words));
        }
        let mut session = Session::new()?;
        let a = session.allocate(None, capacity_words)?;
        let b = session.allocate(None, capacity_words)?;
        let c = session.allocate(None, capacity_words)?;
        let out = session.allocate(None, capacity_words)?;
        let mut kernels = [ptr::null_mut(); 3];
        for (index, op) in [
            PtxOperation::Xor3,
            PtxOperation::Hamming,
            PtxOperation::TernaryPartial,
        ]
        .into_iter()
        .enumerate()
        {
            let symbol =
                CString::new(op.symbol()).map_err(|_| GpuError::Unavailable("PTX symbol"))?;
            // SAFETY: session owns the compiled module; CUDA writes one
            // function handle into the indexed element.
            unsafe {
                check(
                    "cuModuleGetFunction",
                    (session.driver.function)(&mut kernels[index], session.module, symbol.as_ptr()),
                )?;
            }
        }
        Ok(Self {
            session,
            a,
            b,
            c,
            out,
            capacity_words,
            kernels,
        })
    }

    /// Capacity in 32-bit words; allocation never grows implicitly.
    #[must_use]
    pub const fn capacity_words(&self) -> usize {
        self.capacity_words
    }

    /// Upload new inputs into the already allocated A, B, C buffers. No
    /// allocation or JIT occurs. The entire requested region is overwritten.
    ///
    /// # Errors
    /// A mismatch is rejected before any writes to GPU memory.
    pub fn upload(&mut self, a: &[u32], b: &[u32], c: &[u32]) -> Result<(), GpuError> {
        if a.len() != b.len() || a.len() != c.len() {
            return Err(GpuError::Shape(a.len(), b.len(), c.len()));
        }
        if a.len() > self.capacity_words {
            return Err(GpuError::Oversize(a.len()));
        }
        if a.is_empty() {
            return Ok(());
        }
        for (device, host) in [(self.a, a), (self.b, b), (self.c, c)] {
            // SAFETY: every device allocation is at least capacity_words
            // words, the input slice length is checked and host is live.
            unsafe {
                check(
                    "cuMemcpyHtoD_v2",
                    (self.session.driver.h2d)(device, host.as_ptr().cast(), mem::size_of_val(host)),
                )?;
            }
        }
        Ok(())
    }

    fn launch(&self, operation: PtxOperation, used_words: usize) -> Result<(), GpuError> {
        if used_words > self.capacity_words {
            return Err(GpuError::Oversize(used_words));
        }
        if used_words == 0 {
            return Ok(());
        }
        let mut a = self.a;
        let mut b = self.b;
        let mut c = self.c;
        let mut out = self.out;
        let mut n = used_words as u32;
        let selected = match operation {
            PtxOperation::Xor3 => 0,
            PtxOperation::Hamming => 1,
            PtxOperation::TernaryPartial => 2,
        };
        let mut params = [
            (&mut a as *mut DevicePtr).cast::<c_void>(),
            (&mut b as *mut DevicePtr).cast::<c_void>(),
            (&mut c as *mut DevicePtr).cast::<c_void>(),
            (&mut out as *mut DevicePtr).cast::<c_void>(),
            (&mut n as *mut u32).cast::<c_void>(),
        ];
        // SAFETY: all parameters are live through synchronous cuLaunchKernel
        // submission, module and pointers belong to the same CUDA context,
        // and n <= allocated device capacity.
        unsafe {
            check(
                "cuLaunchKernel",
                (self.session.driver.launch)(
                    self.kernels[selected],
                    n.div_ceil(256),
                    1,
                    1,
                    256,
                    1,
                    1,
                    0,
                    ptr::null_mut(),
                    params.as_mut_ptr(),
                    ptr::null_mut(),
                ),
            )?;
        }
        Ok(())
    }

    /// Read the prior result from the resident device output buffer.
    /// Synchronizes the default stream through synchronous cuMemcpyDtoH.
    ///
    /// # Errors
    /// Rejects capacity violations and returns native CUDA copy failures.
    pub fn read_output(&self, used_words: usize) -> Result<Vec<u32>, GpuError> {
        if used_words > self.capacity_words {
            return Err(GpuError::Oversize(used_words));
        }
        let mut result = vec![0_u32; used_words];
        if used_words != 0 {
            // SAFETY: result owns a fully allocated host buffer and device
            // out has capacity_words allocated u32 slots.
            unsafe {
                check(
                    "cuMemcpyDtoH_v2",
                    (self.session.driver.d2h)(
                        result.as_mut_ptr().cast(),
                        self.out,
                        mem::size_of_val(result.as_slice()),
                    ),
                )?;
            }
        }
        Ok(result)
    }

    /// Launch and read back, without reallocating or reloading the module.
    ///
    /// # Errors
    /// Native CUDA failures propagate to the caller.
    pub fn execute(
        &self,
        operation: PtxOperation,
        used_words: usize,
    ) -> Result<Vec<u32>, GpuError> {
        self.launch(operation, used_words)?;
        self.read_output(used_words)
    }

    /// Measure warmed-up device elapsed time for identical data already in
    /// the device buffers, excluding host copies, context creation and PTX JIT.
    /// GPU events surround only repeated kernel launches; host submission
    /// latency and device scheduling can still affect the observed interval.
    ///
    /// # Errors
    /// Rejects zero iterations/empty shapes and native CUDA API failures.
    pub fn benchmark_device(
        &self,
        operation: PtxOperation,
        used_words: usize,
        iterations: u32,
    ) -> Result<GpuKernelTiming, GpuError> {
        if iterations == 0 || used_words == 0 || used_words > self.capacity_words {
            return Err(GpuError::Oversize(used_words));
        }
        self.launch(operation, used_words)?;
        // SAFETY: same-context default stream, warmed-up launch must finish
        // before the measurement events are recorded.
        unsafe { check("cuCtxSynchronize", (self.session.driver.sync)())? };
        let events = TimedEvents::new(&self.session.driver)?;
        events.mark(events.first)?;
        for _ in 0..iterations {
            self.launch(operation, used_words)?;
        }
        events.mark(events.last)?;
        let total_device_ms = events.elapsed_ms()?;
        Ok(GpuKernelTiming {
            iterations,
            total_device_ms,
            device_ms_per_launch: total_device_ms / iterations as f32,
        })
    }
}

#[cfg(test)]
mod resident_tests {
    use super::*;

    #[test]
    fn rejects_invalid_capacity_without_loading_driver() {
        assert!(matches!(ResidentGpu::new(0), Err(GpuError::Oversize(0))));
        assert!(matches!(
            ResidentGpu::new((1 << 24) + 1),
            Err(GpuError::Oversize(_))
        ));
    }

    #[test]
    fn device_buffers_reused_across_payloads_and_ops_when_explicitly_enabled() {
        if std::env::var_os("SML_NATIVE_GPU_TEST").is_none() {
            return;
        }
        let capacity = 1031;
        let mut gpu = ResidentGpu::new(capacity).expect("real NVIDIA GPU required");
        assert_eq!(gpu.capacity_words(), capacity);
        for words in [1, 31, 255, 256, 257, 1031, 19] {
            let a: Vec<u32> = (0..words)
                .map(|i| (i as u32).wrapping_mul(0x9e37_79b9))
                .collect();
            let b: Vec<u32> = (0..words).map(|i| !(i as u32).rotate_left(7)).collect();
            let c: Vec<u32> = (0..words).map(|i| (i as u32).rotate_right(13)).collect();
            gpu.upload(&a, &b, &c).expect("write buffers");
            for op in [
                PtxOperation::Xor3,
                PtxOperation::Hamming,
                PtxOperation::TernaryPartial,
            ] {
                let output = gpu.execute(op, words).expect("device calculation");
                for (index, actual) in output.iter().copied().enumerate() {
                    assert_eq!(
                        actual,
                        crate::ptx_word_oracle(op, a[index], b[index], c[index]),
                        "operation={op:?},words={words},index={index}"
                    );
                }
            }
            let measured = gpu
                .benchmark_device(PtxOperation::Hamming, words, 8)
                .expect("device event timer");
            assert!(measured.total_device_ms.is_finite());
            assert!(measured.total_device_ms >= 0.0);
            assert_eq!(measured.iterations, 8);
        }
        let invalid = gpu.upload(&[0, 1], &[1], &[0, 1]);
        assert_eq!(invalid, Err(GpuError::Shape(2, 1, 2)));
        assert!(matches!(
            gpu.benchmark_device(PtxOperation::Xor3, 1, 0),
            Err(GpuError::Oversize(_))
        ));
    }
}
