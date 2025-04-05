use std::time::Instant;
use std::sync::Arc;
use cudarc::driver::{CudaDevice, LaunchConfig};

// CUDA kernel for keypair generation simulation
const CUDA_SRC: &str = r#"
extern "C" __global__ void generate_keypairs(unsigned char *keys, int num_keys) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx < num_keys) {
        // Fill with random-like data based on thread id and time
        // In a real implementation, this would be actual keypair generation
        int offset = idx * 32;
        for (int i = 0; i < 32; i++) {
            keys[offset + i] = (unsigned char)((idx + i) % 256);
        }
    }
}
"#;

/// Benchmark results with statistical information
pub struct BenchmarkResult {
    pub keys_per_second: f64,
    pub confidence_interval: (f64, f64),
    pub samples: Vec<f64>,
    pub optimal_batch_size: usize,
}

/// Initialize the CUDA device
fn init_cuda() -> Result<CudaDevice, String> {
    match CudaDevice::new(0) {
        Ok(dev) => {
            // Load PTX code
            match dev.load_ptx(CUDA_SRC.into(), "benchmark_kernels", &["generate_keypairs"]) {
                Ok(_) => Ok(dev),
                Err(e) => Err(format!("Failed to load CUDA kernel: {}", e))
            }
        },
        Err(e) => Err(format!("Failed to initialize CUDA device: {}", e))
    }
}

/// Run a single GPU benchmark with a specific batch size
fn benchmark_batch_gpu(dev: &CudaDevice, batch_size: usize) -> Result<f64, String> {
    // Allocate memory on GPU
    let keys_buffer = match dev.alloc_zeros::<u8>(batch_size * 32) {
        Ok(buf) => buf,
        Err(e) => return Err(format!("Failed to allocate GPU memory: {}", e))
    };

    let start = Instant::now();
    
    // Get function
    let func = match dev.get_func("benchmark_kernels", "generate_keypairs") {
        Some(f) => f,
        None => return Err("Could not find generate_keypairs kernel function".to_string())
    };
    
    // Configure block and grid dimensions
    let threads_per_block = 256;
    let blocks = (batch_size as u32 + threads_per_block - 1) / threads_per_block;
    
    let config = LaunchConfig {
        grid_dim: (blocks as u32, 1, 1),
        block_dim: (threads_per_block, 1, 1),
        shared_mem_bytes: 0,
    };
    
    // Convert batch_size to a device-accessible value
    let batch_size_gpu = match dev.htod_copy(vec![batch_size as i32]) {
        Ok(buf) => buf,
        Err(e) => return Err(format!("Failed to copy batch size to GPU: {}", e))
    };
    
    // Launch kernel
    if let Err(e) = unsafe { func.launch(config, &[&keys_buffer, &batch_size_gpu]) } {
        return Err(format!("Kernel launch failed: {}", e));
    }
    
    // Wait for GPU to finish
    if let Err(e) = dev.synchronize() {
        return Err(format!("Failed to synchronize GPU: {}", e));
    }
    
    let duration = start.elapsed();
    Ok(batch_size as f64 / duration.as_secs_f64())
}

/// Calculate mean and standard deviation
fn calculate_statistics(samples: &[f64]) -> (f64, f64) {
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let variance = samples.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / samples.len() as f64;
    let std_dev = variance.sqrt();
    
    (mean, std_dev)
}

/// Calculate 95% confidence interval
fn confidence_interval(mean: f64, std_dev: f64, sample_size: usize) -> (f64, f64) {
    // Using 1.96 for 95% confidence
    let margin = 1.96 * std_dev / (sample_size as f64).sqrt();
    (mean - margin, mean + margin)
}

/// Check if measurements have stabilized
fn is_stable(samples: &[f64], threshold: f64) -> bool {
    if samples.len() < 3 {
        return false;
    }
    
    let (mean, std_dev) = calculate_statistics(samples);
    let coefficient_of_variation = std_dev / mean;
    
    coefficient_of_variation < threshold
}

/// Find the optimal batch size for benchmarking
fn find_optimal_batch_size(dev: &CudaDevice) -> Result<usize, String> {
    // For CUDA, we need much larger batch sizes to saturate the GPU
    let batch_sizes = [100_000, 500_000, 1_000_000, 5_000_000, 10_000_000];
    let mut best_size = batch_sizes[0];
    let mut best_rate = 0.0;
    
    for &size in &batch_sizes {
        println!("Testing batch size: {}", size);
        match benchmark_batch_gpu(dev, size) {
            Ok(rate) => {
                println!("  Rate: {:.2} keys/s", rate);
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
                println!("  Error with batch size {}: {}", size, e);
                // If this batch size failed, don't try larger ones
                break;
            }
        }
    }
    
    if best_rate == 0.0 {
        return Err("Could not find a working batch size".to_string());
    }
    
    Ok(best_size)
}

/// Run GPU benchmarks to determine the system's true capability
pub fn run_comprehensive_benchmark(iterations: usize, stability_threshold: f64) -> Result<BenchmarkResult, String> {
    println!("Initializing CUDA...");
    let dev = match init_cuda() {
        Ok(dev) => dev,
        Err(e) => return Err(format!("CUDA initialization failed: {}. Falling back to CPU benchmark.", e))
    };
    
    println!("Finding optimal batch size for GPU benchmarking...");
    let optimal_batch_size = match find_optimal_batch_size(&dev) {
        Ok(size) => size,
        Err(e) => return Err(format!("Failed to find optimal batch size: {}", e))
    };
    println!("Optimal batch size: {}", optimal_batch_size);
    
    let mut samples = Vec::new();
    
    println!("Running GPU benchmark with up to {} iterations...", iterations);
    println!("Will stop early if measurements stabilize");
    
    // Run benchmarks until we have stable measurements or reach max iterations
    for i in 0..iterations {
        match benchmark_batch_gpu(&dev, optimal_batch_size) {
            Ok(rate) => {
                samples.push(rate);
                
                let (mean, std_dev) = calculate_statistics(&samples);
                println!("Iteration {}: {:.2} keys/s (±{:.2})", i + 1, mean, std_dev);
                
                if samples.len() >= 3 && is_stable(&samples, stability_threshold) {
                    println!("Measurements have stabilized after {} iterations", i + 1);
                    break;
                }
            },
            Err(e) => {
                return Err(format!("Benchmark iteration {} failed: {}", i + 1, e));
            }
        }
    }
    
    // Calculate final statistics
    let (mean, std_dev) = calculate_statistics(&samples);
    let ci = confidence_interval(mean, std_dev, samples.len());
    
    // Use the lower bound of the confidence interval for conservative estimates
    let conservative_rate = ci.0;
    
    Ok(BenchmarkResult {
        keys_per_second: conservative_rate,
        confidence_interval: ci,
        samples,
        optimal_batch_size,
    })
}

/// Helper function to format the benchmark results in a human-readable way
pub fn format_benchmark_result(result: &BenchmarkResult) -> String {
    format!(
        "CUDA GPU Performance: {:.2} keys/second\n\
         95% confidence interval: ({:.2}, {:.2})\n\
         Optimal batch size: {}\n\
         Sample count: {}\n\
         Sample variance: {:.2}%",
        result.keys_per_second,
        result.confidence_interval.0,
        result.confidence_interval.1,
        result.optimal_batch_size,
        result.samples.len(),
        calculate_statistics(&result.samples).1 / calculate_statistics(&result.samples).0 * 100.0
    )
}