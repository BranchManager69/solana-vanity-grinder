use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::ptr;
use std::str;
use std::time::Instant;
use colored::*;

use cuda_driver_sys::*;
use cuda_runtime_sys::*;

// Include the PTX code generated during build
include!(concat!(env!("OUT_DIR"), "/cuda_ptx.rs"));

/// Wrapper for CUDA errors
#[derive(Debug)]
pub enum CudaError {
    Driver(CUresult),
    Runtime(cudaError_t),
    Other(String),
    SignatureError(String),
}

impl std::fmt::Display for CudaError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            CudaError::Driver(code) => write!(f, "CUDA driver error: {}", cuda_driver_error_to_str(*code)),
            CudaError::Runtime(code) => write!(f, "CUDA runtime error: {}", cuda_runtime_error_to_str(*code)),
            CudaError::Other(msg) => write!(f, "CUDA error: {}", msg),
            CudaError::SignatureError(e) => write!(f, "Signature error: {}", e),
        }
    }
}

impl std::error::Error for CudaError {}

impl From<solana_sdk::signature::SignerError> for CudaError {
    fn from(e: solana_sdk::signature::SignerError) -> Self {
        CudaError::SignatureError(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CudaError>;

/// Convert CUDA driver error code to string
fn cuda_driver_error_to_str(error: CUresult) -> String {
    unsafe {
        let mut string_ptr: *const c_char = ptr::null();
        cuGetErrorString(error, &mut string_ptr);
        if string_ptr.is_null() {
            return format!("Unknown CUDA driver error: {:?}", error);
        }
        CStr::from_ptr(string_ptr).to_string_lossy().into_owned()
    }
}

/// Convert CUDA runtime error code to string
fn cuda_runtime_error_to_str(error: cudaError_t) -> String {
    unsafe {
        let string_ptr = cudaGetErrorString(error);
        if string_ptr.is_null() {
            return format!("Unknown CUDA runtime error: {:?}", error);
        }
        CStr::from_ptr(string_ptr).to_string_lossy().into_owned()
    }
}

/// Check CUDA driver API error
fn check_cu(result: CUresult) -> Result<()> {
    // CUDA_SUCCESS = 0
    if result as i32 != 0 {
        Err(CudaError::Driver(result))
    } else {
        Ok(())
    }
}

/// Check CUDA runtime API error
fn check_rt(result: cudaError_t) -> Result<()> {
    // cudaSuccess = 0
    if result as i32 != 0 {
        Err(CudaError::Runtime(result))
    } else {
        Ok(())
    }
}

/// CUDA device information
#[derive(Debug)]
pub struct CudaDevice {
    device: CUdevice,
    context: CUcontext,
    module: CUmodule,
    function: CUfunction,
    name: String,
    compute_capability: (i32, i32),
    total_memory: usize,
}

// Safety: These are manually managed GPU resources and we ensure proper synchronization
unsafe impl Send for CudaDevice {}
unsafe impl Sync for CudaDevice {}

impl CudaDevice {
    /// Get the CUDA module
    pub fn get_module(&self) -> CUmodule {
        self.module
    }
}

impl CudaDevice {
    /// Initialize CUDA and get device info
    pub fn new() -> Result<Self> {
        unsafe {
            // Initialize CUDA driver API
            check_cu(cuInit(0))?;
            
            // Get first CUDA device
            let mut device = 0;
            check_cu(cuDeviceGet(&mut device, 0))?;
            
            // Get device name
            let mut name_raw = [0 as c_char; 256];
            check_cu(cuDeviceGetName(name_raw.as_mut_ptr(), name_raw.len() as i32, device))?;
            let name = CStr::from_ptr(name_raw.as_ptr())
                .to_string_lossy()
                .into_owned();
            
            // Get compute capability
            let mut major = 0;
            let mut minor = 0;
            
            // Using enum values directly cast to the proper type
            check_cu(cuDeviceGetAttribute(&mut major, 
                CUdevice_attribute_enum::CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR, device))?;
            check_cu(cuDeviceGetAttribute(&mut minor, 
                CUdevice_attribute_enum::CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR, device))?;
            
            // Get total memory
            let mut total_memory = 0;
            check_cu(cuDeviceTotalMem_v2(&mut total_memory, device))?;
            
            // Create CUDA context
            let mut context = ptr::null_mut();
            check_cu(cuCtxCreate_v2(&mut context, 0, device))?;
            
            // Load PTX module
            let ptx_cstring = CString::new(PTX_SRC).map_err(|e| CudaError::Other(e.to_string()))?;
            let mut module = ptr::null_mut();
            check_cu(cuModuleLoadData(&mut module, ptx_cstring.as_ptr() as *const c_void))?;
            
            // Get function from module
            let func_name = CString::new("generate_keypairs").map_err(|e| CudaError::Other(e.to_string()))?;
            let mut function = ptr::null_mut();
            check_cu(cuModuleGetFunction(&mut function, module, func_name.as_ptr()))?;
            
            Ok(CudaDevice {
                device,
                context,
                module,
                function,
                name,
                compute_capability: (major, minor),
                total_memory: total_memory as usize,
            })
        }
    }
    
    /// Get device info as formatted string
    pub fn get_info(&self) -> String {
        let name = self.name.bright_green().bold();
        let compute = format!("{}.{}", self.compute_capability.0, self.compute_capability.1).cyan();
        let memory = format!("{} MB", self.total_memory / (1024 * 1024)).cyan();
        
        format!(
            "{}: {}\n\
             {}: {}\n\
             {}: {}",
            "CUDA Device".blue().bold(), name,
            "Compute Capability".blue().bold(), compute,
            "Total Memory".blue().bold(), memory
        )
    }
    
    /// Allocate device memory
    pub fn alloc(&self, size: usize) -> Result<CUdeviceptr> {
        unsafe {
            let mut ptr = 0;
            check_cu(cuMemAlloc_v2(&mut ptr, size))?;
            Ok(ptr)
        }
    }
    
    /// Free device memory
    pub fn free(&self, ptr: CUdeviceptr) -> Result<()> {
        unsafe {
            check_cu(cuMemFree_v2(ptr))
        }
    }
    
    /// Copy memory from host to device
    pub fn copy_htod(&self, device_ptr: CUdeviceptr, host_ptr: *const c_void, size: usize) -> Result<()> {
        unsafe {
            check_cu(cuMemcpyHtoD_v2(device_ptr, host_ptr, size))
        }
    }
    
    /// Copy memory from device to host
    pub fn copy_dtoh(&self, host_ptr: *mut c_void, device_ptr: CUdeviceptr, size: usize) -> Result<()> {
        unsafe {
            check_cu(cuMemcpyDtoH_v2(host_ptr, device_ptr, size))
        }
    }
    
    /// Launch CUDA kernel
    pub fn launch_kernel(
        &self,
        grid_dim: (c_uint, c_uint, c_uint),
        block_dim: (c_uint, c_uint, c_uint),
        args: &mut [*mut c_void],
    ) -> Result<()> {
        unsafe {
            let args_ptr = args.as_mut_ptr();
            check_cu(cuLaunchKernel(
                self.function,
                grid_dim.0, grid_dim.1, grid_dim.2,
                block_dim.0, block_dim.1, block_dim.2,
                0, // Shared memory bytes
                ptr::null_mut(), // Stream
                args_ptr,
                ptr::null_mut(), // Extra
            ))
        }
    }
    
    /// Synchronize device
    pub fn synchronize(&self) -> Result<()> {
        unsafe {
            check_cu(cuCtxSynchronize())
        }
    }
}

impl Drop for CudaDevice {
    fn drop(&mut self) {
        unsafe {
            cuModuleUnload(self.module);
            cuCtxDestroy_v2(self.context);
        }
    }
}

/// Benchmark to measure keypair generation performance
pub struct BenchmarkResult {
    pub keys_per_second: f64,
    pub confidence_interval: (f64, f64),
    pub samples: Vec<f64>,
    pub optimal_batch_size: usize,
}

/// Format a number with commas
fn format_number(num: f64) -> String {
    let num_int = num.round() as u64;
    let mut s = String::new();
    let digits = num_int.to_string();
    let len = digits.len();
    
    for (i, c) in digits.chars().enumerate() {
        s.push(c);
        if (len - i - 1) % 3 == 0 && i < len - 1 {
            s.push(',');
        }
    }
    s
}

/// Run a single benchmark with a specific batch size
pub fn benchmark_batch_gpu(device: &CudaDevice, batch_size: usize) -> Result<f64> {
    let key_size = 32;
    let total_bytes = batch_size * key_size;
    
    // Allocate device memory for keys
    let d_keys = device.alloc(total_bytes)?;
    
    // Set up kernel launch parameters
    let threads_per_block = 256;
    let blocks = (batch_size as u32 + threads_per_block - 1) / threads_per_block;
    
    // Prepare kernel arguments
    let batch_size_int = batch_size as c_int;
    let mut args = [
        &d_keys as *const _ as *mut c_void,
        &batch_size_int as *const _ as *mut c_void,
    ];
    
    // Run kernel and measure time
    let start = Instant::now();
    
    device.launch_kernel(
        (blocks, 1, 1),
        (threads_per_block, 1, 1),
        &mut args,
    )?;
    
    device.synchronize()?;
    
    let duration = start.elapsed();
    
    // Clean up
    device.free(d_keys)?;
    
    Ok(batch_size as f64 / duration.as_secs_f64())
}

/// Find the optimal batch size for benchmarking
pub fn find_optimal_batch_size(device: &CudaDevice) -> Result<usize> {
    // For CUDA, we need much larger batch sizes to saturate the GPU
    let batch_sizes = [100_000, 500_000, 1_000_000, 5_000_000, 10_000_000];
    let mut best_size = batch_sizes[0];
    let mut best_rate = 0.0;
    
    for &size in &batch_sizes {
        // Print in dark gray
        println!("{} {}", "Testing batch size:".truecolor(150, 150, 150), 
                 format_number(size as f64).truecolor(150, 150, 150));
        
        match benchmark_batch_gpu(device, size) {
            Ok(rate) => {
                // Print in light gray
                println!("  {}: {} {}", "Rate".truecolor(200, 200, 200), 
                         format_number(rate).truecolor(200, 200, 200), 
                         "keys/s".truecolor(200, 200, 200));
                
                if rate > best_rate {
                    best_rate = rate;
                    best_size = size;
                }
                
                // If we've found a good batch size and we're within 95% of the best rate,
                // prefer the smaller batch size for faster iterations
                if rate > 0.95 * best_rate && size < best_size {
                    best_size = size;
                }
            },
            Err(e) => {
                println!("  {}: {}", "Error with batch size".red().bold(), format_number(size as f64));
                println!("  {}", e.to_string().red());
                // If this batch size failed, don't try larger ones
                break;
            }
        }
    }
    
    if best_rate == 0.0 {
        return Err(CudaError::Other("Could not find a working batch size".to_string()));
    }
    
    Ok(best_size)
}