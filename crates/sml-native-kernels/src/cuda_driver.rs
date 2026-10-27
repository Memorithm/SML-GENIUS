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
type FnLaunch = unsafe extern "C" fn(
    Function, u32, u32, u32, u32, u32, u32, u32,
    *mut c_void, *mut *mut c_void, *mut *mut c_void
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
    if result == 0 { Ok(()) } else { Err(GpuError::Driver(name, result)) }
}

/// # Safety
/// Caller guarantees the C ABI type corresponds to this fixed CUDA symbol,
/// and keeps the library loaded until all uses of the returned pointer end.
unsafe fn load<T: Copy>(
    library: *mut c_void, name: &std::ffi::CStr, label: &'static str
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
            check("cuCtxCreate_v2", (session.driver.ctx_create)(&mut session.context, 0, device))?;
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
            for device in self.buffers.drain(..) { (self.driver.free)(device); }
            if !self.module.is_null() { (self.driver.module_unload)(self.module); }
            if !self.context.is_null() { (self.driver.ctx_destroy)(self.context); }
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
    operation: PtxOperation, a: &[u32], b: &[u32], c: &[u32]
) -> Result<Vec<u32>, GpuError> {
    if a.len() != b.len() || a.len() != c.len() {
        return Err(GpuError::Shape(a.len(), b.len(), c.len()));
    }
    if a.len() > (1 << 24) { return Err(GpuError::Oversize(a.len())); }
    if a.is_empty() { return Ok(Vec::new()); }

    let mut session = Session::new()?;
    let mut input_a = session.allocate(Some(a), a.len())?;
    let mut input_b = session.allocate(Some(b), b.len())?;
    let mut input_c = session.allocate(Some(c), c.len())?;
    let mut output = session.allocate(None, a.len())?;
    let mut length = a.len() as u32;
    let mut function: Function = ptr::null_mut();
    let symbol = CString::new(operation.symbol()).map_err(|_| GpuError::Unavailable("symbol NUL"))?;
    // SAFETY: live module/function; each pointer points to a live CUDA
    // kernel parameter for the full duration of the synchronous launch.
    unsafe {
        check("cuModuleGetFunction", (session.driver.function)(
            &mut function, session.module, symbol.as_ptr()
        ))?;
        let mut parameters = [
            (&mut input_a as *mut DevicePtr).cast::<c_void>(),
            (&mut input_b as *mut DevicePtr).cast::<c_void>(),
            (&mut input_c as *mut DevicePtr).cast::<c_void>(),
            (&mut output as *mut DevicePtr).cast::<c_void>(),
            (&mut length as *mut u32).cast::<c_void>(),
        ];
        check("cuLaunchKernel", (session.driver.launch)(
            function, length.div_ceil(256), 1, 1, 256, 1, 1,
            0, ptr::null_mut(), parameters.as_mut_ptr(), ptr::null_mut()
        ))?;
        check("cuCtxSynchronize", (session.driver.sync)())?;
    }
    let mut result = vec![0_u32; a.len()];
    // SAFETY: GPU kernel has completed, destination is an initialized,
    // exclusively owned Vec of a.len() elements and allocation matches size.
    unsafe {
        check("cuMemcpyDtoH_v2", (session.driver.d2h)(
            result.as_mut_ptr().cast(), output, mem::size_of_val(result.as_slice())
        ))?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ptx_word_oracle;

    #[test]
    fn rejects_invalid_lengths_without_cuda_or_allocation() {
        assert_eq!(execute(PtxOperation::Hamming, &[1, 2], &[0], &[1, 2]),
            Err(GpuError::Shape(2, 1, 2)));
        assert_eq!(execute(PtxOperation::Xor3, &[], &[], &[]), Ok(Vec::new()));
    }

    #[test]
    fn exact_real_gpu_vectors_only_when_requested() {
        if std::env::var_os("SML_NATIVE_GPU_TEST").is_none() { return; }
        let n = 1031;
        let a: Vec<u32> = (0..n).map(|x| (x as u32).wrapping_mul(0x9e37_79b9)).collect();
        let b: Vec<u32> = (0..n).map(|x| !(x as u32).rotate_left(7)).collect();
        let c: Vec<u32> = (0..n).map(|x| (x as u32).rotate_right(13)).collect();
        for op in [PtxOperation::Xor3, PtxOperation::Hamming, PtxOperation::TernaryPartial] {
            let observed = execute(op, &a, &b, &c).expect("GPU required by SML_NATIVE_GPU_TEST");
            let expected: Vec<_> = a.iter().zip(&b).zip(&c)
                .map(|((&x, &y), &z)| ptx_word_oracle(op, x, y, z)).collect();
            assert_eq!(observed, expected, "operation={op:?}");
        }
    }
}
