use std::ffi::CString;
use std::os::raw::{c_void, c_int, c_char};
use std::ptr;
use std::str;
use std::time::{Duration, Instant};
use colored::*;
use rand::rngs::OsRng;
use rand::Rng;
use solana_sdk::signature::{Keypair, Signer};
use crate::cuda_helpers::{CudaDevice, CudaError, Result};

/// Generator mode for vanity addresses
pub enum VanityMode {
    /// Match at the beginning of the address
    Prefix,
    /// Match at the end of the address
    Suffix,
    /// Match at specific position (index, pattern)
    Position(usize, String),
    /// Match anywhere in the address
    Contains,
}

/// Result of a vanity address search
pub struct VanityResult {
    /// The found keypair
    pub keypair: Keypair,
    /// The address of the keypair
    pub address: String,
    /// How long the search took
    pub duration: Duration,
    /// How many attempts were made
    pub attempts: u64,
}

/// Find the optimal batch size for the GPU
pub fn find_optimal_batch_size(device: &CudaDevice) -> Result<usize> {
    // For CUDA, we need much larger batch sizes to saturate the GPU
    let batch_sizes = [100_000, 500_000, 1_000_000, 5_000_000, 10_000_000];
    let mut best_size = batch_sizes[0];
    let mut best_rate = 0.0;
    
    for &size in &batch_sizes {
        // Print in dark gray
        println!("{} {}", "Testing batch size:".truecolor(150, 150, 150), 
                 format_number(size as f64).truecolor(150, 150, 150));
        
        match crate::cuda_helpers::benchmark_batch_gpu(device, size) {
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

/// Generate a vanity address matching the specified pattern
pub fn generate_vanity_address(
    device: &CudaDevice,
    pattern: &str,
    mode: VanityMode,
    case_sensitive: bool,
    batch_size: usize,
    max_attempts: Option<u64>,
) -> Result<VanityResult> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    
    // Create a dummy stop flag that is never set
    let stop_flag = Arc::new(AtomicBool::new(false));
    
    // Call the version with progress updates, but with a dummy callback
    generate_vanity_address_with_updates(
        device,
        pattern,
        mode,
        case_sensitive,
        batch_size,
        max_attempts,
        stop_flag,
        |_| {}
    )
}

/// Generate a vanity address with progress updates and cancellation support
pub fn generate_vanity_address_with_updates(
    device: &CudaDevice,
    pattern: &str,
    mode: VanityMode,
    case_sensitive: bool,
    batch_size: usize,
    max_attempts: Option<u64>,
    stop_flag: Arc<AtomicBool>,
    update_callback: impl Fn(u64) + Send,
) -> Result<VanityResult> {
    // Pattern validation (Solana addresses use base58 alphabet)
    for c in pattern.chars() {
        if !"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(c) {
            return Err(CudaError::Other(format!("Invalid character '{}' in pattern. Only Base58 characters are allowed.", c)));
        }
    }

    // Convert pattern to CString for passing to kernel
    let pattern_cstring = match CString::new(pattern) {
        Ok(cs) => cs,
        Err(_) => return Err(CudaError::Other("Invalid pattern string".to_string())),
    };

    // Determine if we're searching by prefix
    let is_prefix = match mode {
        VanityMode::Prefix => true,
        VanityMode::Suffix => false,
        _ => return Err(CudaError::Other("Only prefix and suffix modes are currently supported".to_string())),
    };

    // Allocate device memory for various data structures
    let states_size = batch_size * std::mem::size_of::<u64>() * 8; // Estimate for curandState
    let d_states = device.alloc(states_size)?;
    
    let key_size = 32; // Ed25519 seed size
    let seeds_size = batch_size * key_size;
    let d_seeds = device.alloc(seeds_size)?;
    
    let d_result = device.alloc(key_size)?;
    let d_found_flag = device.alloc(std::mem::size_of::<c_int>())?;

    // Zero out the found flag
    let found_flag: c_int = 0;
    unsafe {
        device.copy_htod(d_found_flag, &found_flag as *const _ as *const c_void, std::mem::size_of::<c_int>())?;
    }

    // Copy pattern to device
    let pattern_bytes = pattern_cstring.as_bytes_with_nul();
    let d_pattern = device.alloc(pattern_bytes.len())?;
    unsafe {
        device.copy_htod(d_pattern, pattern_bytes.as_ptr() as *const c_void, pattern_bytes.len())?;
    }

    // Initialize random number generator states
    let mut rng = OsRng;
    let random_seed: u64 = rng.gen();
    
    // Setup init RNG kernel parameters
    let threads_per_block = 256;
    let blocks = (batch_size as u32 + threads_per_block - 1) / threads_per_block;
    let batch_size_int = batch_size as c_int;

    // Get init_rng function from the module
    let init_rng_name = CString::new("init_rng").map_err(|e| CudaError::Other(e.to_string()))?;
    let init_rng_fn = unsafe {
        let mut function = ptr::null_mut();
        // Get function from the module
        match cuda_driver_sys::cuModuleGetFunction(&mut function, device.get_module(), init_rng_name.as_ptr()) {
            0 => function,
            error => return Err(CudaError::Driver(error)),
        }
    };

    // Prepare and launch init_rng kernel
    let mut init_args = [
        &d_states as *const _ as *mut c_void,
        &random_seed as *const _ as *mut c_void,
        &batch_size_int as *const _ as *mut c_void,
    ];

    unsafe {
        // Launch init_rng kernel
        match cuda_driver_sys::cuLaunchKernel(
            init_rng_fn,
            blocks, 1, 1,
            threads_per_block, 1, 1,
            0, // Shared memory bytes
            ptr::null_mut(), // Stream
            init_args.as_mut_ptr(),
            ptr::null_mut(), // Extra
        ) {
            0 => (),
            error => return Err(CudaError::Driver(error)),
        }
        
        // Wait for init kernel to finish
        device.synchronize()?;
    }

    // Get generate_and_check_keypairs function from the module
    let gen_name = CString::new("generate_and_check_keypairs").map_err(|e| CudaError::Other(e.to_string()))?;
    let gen_fn = unsafe {
        let mut function = ptr::null_mut();
        // Get function from the module
        match cuda_driver_sys::cuModuleGetFunction(&mut function, device.get_module(), gen_name.as_ptr()) {
            0 => function,
            error => return Err(CudaError::Driver(error)),
        }
    };

    // Prepare parameters for the main kernel
    let pattern_len = pattern.len() as c_int;
    let is_prefix_int = if is_prefix { 1 } else { 0 };
    let case_sensitive_int = if case_sensitive { 1 } else { 0 };

    // Prepare kernel arguments
    let mut gen_args = [
        &d_states as *const _ as *mut c_void,
        &d_seeds as *const _ as *mut c_void,
        &d_result as *const _ as *mut c_void,
        &d_found_flag as *const _ as *mut c_void,
        &d_pattern as *const _ as *mut c_void,
        &pattern_len as *const _ as *mut c_void,
        &is_prefix_int as *const _ as *mut c_void,
        &case_sensitive_int as *const _ as *mut c_void,
        &batch_size_int as *const _ as *mut c_void,
    ];

    // Start timing and searching
    let start_time = Instant::now();
    let mut found = false;
    let mut attempts = 0u64;
    let max_attempts = max_attempts.unwrap_or(u64::MAX);

    while !found && attempts < max_attempts {
        // Check if we should stop (for cancellation)
        if stop_flag.load(Ordering::Relaxed) {
            unsafe {
                device.free(d_states)?;
                device.free(d_seeds)?;
                device.free(d_result)?;
                device.free(d_found_flag)?;
                device.free(d_pattern)?;
            }
            return Err(CudaError::Other("Search was cancelled".to_string()));
        }
        
        // Update progress every 5 iterations
        if attempts % (5 * batch_size as u64) == 0 {
            // Call the update callback with current progress
            update_callback(attempts);
            
            // For CLI version, print progress
            if attempts > 0 {
                let elapsed = start_time.elapsed();
                let rate = attempts as f64 / elapsed.as_secs_f64();
                
                println!("{}",
                         format!("⚡ Searching... {} addresses checked ({}/s)",
                                 format_number(attempts as f64),
                                 format_number(rate))
                         .bright_blue());
            }
        }

        unsafe {
            // Launch the main kernel
            match cuda_driver_sys::cuLaunchKernel(
                gen_fn,
                blocks, 1, 1,
                threads_per_block, 1, 1,
                0, // Shared memory bytes
                ptr::null_mut(), // Stream
                gen_args.as_mut_ptr(),
                ptr::null_mut(), // Extra
            ) {
                0 => (),
                error => return Err(CudaError::Driver(error)),
            }
            
            // Wait for kernel to finish
            device.synchronize()?;
            
            // Check if we found a match
            let mut found_flag_result: c_int = 0;
            device.copy_dtoh(&mut found_flag_result as *mut _ as *mut c_void, d_found_flag, std::mem::size_of::<c_int>())?;
            
            found = found_flag_result > 0;
            attempts += batch_size as u64;
        }
    }

    // Final progress update
    update_callback(attempts);

    // If found, retrieve the result
    if found {
        let duration = start_time.elapsed();
        
        // Get the result keypair data
        let mut result_bytes = [0u8; 32];
        unsafe {
            device.copy_dtoh(result_bytes.as_mut_ptr() as *mut c_void, d_result, key_size)?;
        }
        
        // Create a Solana keypair from the seed
        let keypair = Keypair::from_bytes(&result_bytes)?;
        let address = keypair.pubkey().to_string();
        
        // Clean up device memory
        unsafe {
            device.free(d_states)?;
            device.free(d_seeds)?;
            device.free(d_result)?;
            device.free(d_found_flag)?;
            device.free(d_pattern)?;
        }
        
        Ok(VanityResult {
            keypair,
            address,
            duration,
            attempts,
        })
    } else {
        // Clean up device memory
        unsafe {
            device.free(d_states)?;
            device.free(d_seeds)?;
            device.free(d_result)?;
            device.free(d_found_flag)?;
            device.free(d_pattern)?;
        }
        
        Err(CudaError::Other("Maximum attempts reached without finding a match".to_string()))
    }
}

/// Format a number with commas for better readability
pub fn format_number(num: f64) -> String {
    if num >= 1_000_000.0 {
        // For large numbers, use no decimal places
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
    } else if num >= 1_000.0 {
        // For medium numbers, use one decimal place
        let mut s = String::new();
        let num_rounded = (num * 10.0).round() / 10.0;
        let digits = format!("{:.1}", num_rounded);
        let parts: Vec<&str> = digits.split('.').collect();
        
        // Format integer part with commas
        let int_part = parts[0];
        let len = int_part.len();
        
        for (i, c) in int_part.chars().enumerate() {
            s.push(c);
            if (len - i - 1) % 3 == 0 && i < len - 1 {
                s.push(',');
            }
        }
        
        // Add decimal part if present
        if parts.len() > 1 && parts[1] != "0" {
            s.push('.');
            s.push_str(parts[1]);
        }
        
        s
    } else {
        // For small numbers, use two decimal places
        format!("{:.2}", num)
    }
}