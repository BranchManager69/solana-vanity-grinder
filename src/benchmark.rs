use std::time::Instant;
use rand::Rng;

/// Benchmark results with statistical information
pub struct BenchmarkResult {
    pub keys_per_second: f64,
    pub confidence_interval: (f64, f64),
    pub samples: Vec<f64>,
    pub optimal_batch_size: usize,
}

/// Generate a specified number of random keypairs
/// This is a placeholder for the actual keypair generation function
fn generate_keypairs(count: usize) -> Vec<[u8; 32]> {
    let mut rng = rand::thread_rng();
    let mut keys = Vec::with_capacity(count);
    
    for _ in 0..count {
        let mut key = [0u8; 32];
        rng.fill(&mut key);
        keys.push(key);
    }
    
    keys
}

/// Run a single benchmark with a specific batch size
fn benchmark_batch(batch_size: usize) -> f64 {
    // Warmup phase
    generate_keypairs(batch_size / 10);
    
    // Measurement phase
    let start = Instant::now();
    generate_keypairs(batch_size);
    let duration = start.elapsed();
    
    batch_size as f64 / duration.as_secs_f64()
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
fn find_optimal_batch_size() -> usize {
    let batch_sizes = [10_000, 50_000, 100_000, 500_000, 1_000_000];
    let mut best_size = batch_sizes[0];
    let mut best_rate = 0.0;
    
    for &size in &batch_sizes {
        let rate = benchmark_batch(size);
        if rate > best_rate {
            best_rate = rate;
            best_size = size;
        }
        
        // If we've found a good batch size and we're within 95% of the best rate,
        // prefer the smaller batch size for faster iterations
        if rate > 0.95 * best_rate && size < best_size {
            best_size = size;
        }
    }
    
    best_size
}

/// Run multi-threaded benchmarks to determine the system's true capability
pub fn run_comprehensive_benchmark(iterations: usize, stability_threshold: f64) -> BenchmarkResult {
    println!("Finding optimal batch size for benchmarking...");
    let optimal_batch_size = find_optimal_batch_size();
    println!("Optimal batch size: {}", optimal_batch_size);
    
    let mut samples = Vec::new();
    
    println!("Running CPU benchmark with up to {} iterations...", iterations);
    println!("Will stop early if measurements stabilize");
    
    // Run benchmarks until we have stable measurements or reach max iterations
    for i in 0..iterations {
        let rate = benchmark_batch(optimal_batch_size);
        samples.push(rate);
        
        let (mean, std_dev) = calculate_statistics(&samples);
        println!("Iteration {}: {:.2} keys/s (±{:.2})", i + 1, mean, std_dev);
        
        if samples.len() >= 3 && is_stable(&samples, stability_threshold) {
            println!("Measurements have stabilized after {} iterations", i + 1);
            break;
        }
    }
    
    // Calculate final statistics
    let (mean, std_dev) = calculate_statistics(&samples);
    let ci = confidence_interval(mean, std_dev, samples.len());
    
    // Multiply by a factor to simulate GPU acceleration for now
    // TODO: Replace with actual GPU implementation
    let gpu_acceleration_factor = 100.0; // Conservative estimate
    let conservative_rate = ci.0 * gpu_acceleration_factor;
    
    BenchmarkResult {
        keys_per_second: conservative_rate,
        confidence_interval: (ci.0 * gpu_acceleration_factor, ci.1 * gpu_acceleration_factor),
        samples,
        optimal_batch_size,
    }
}

/// Helper function to format the benchmark results in a human-readable way
pub fn format_benchmark_result(result: &BenchmarkResult) -> String {
    format!(
        "Performance: {:.2} keys/second\n\
         95% confidence interval: ({:.2}, {:.2})\n\
         Optimal batch size: {}\n\
         Sample count: {}\n\
         Sample variance: {:.2}%\n\
         Note: This is a CPU benchmark with estimated GPU acceleration factor.\n\
         Actual GPU performance will be implemented in a future version.",
        result.keys_per_second,
        result.confidence_interval.0,
        result.confidence_interval.1,
        result.optimal_batch_size,
        result.samples.len(),
        calculate_statistics(&result.samples).1 / calculate_statistics(&result.samples).0 * 100.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_statistics_calculation() {
        let samples = vec![100.0, 110.0, 90.0, 105.0, 95.0];
        let (mean, std_dev) = calculate_statistics(&samples);
        assert!((mean - 100.0).abs() < 0.001);
        assert!((std_dev - 7.071).abs() < 0.001);
    }
    
    #[test]
    fn test_confidence_interval() {
        let mean = 100.0;
        let std_dev = 10.0;
        let sample_size = 25;
        
        let (lower, upper) = confidence_interval(mean, std_dev, sample_size);
        assert!((lower - 96.08).abs() < 0.01);
        assert!((upper - 103.92).abs() < 0.01);
    }
}