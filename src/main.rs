mod estimator;
mod cuda_helpers;
mod cuda;
mod api;

use std::env;
use std::process;
use std::fs;
use std::path::Path;
use std::io::{self, Write};
use std::time::Instant;
use colored::*;
use cuda::vanity_generator::{generate_vanity_address, VanityMode, format_number};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(name = "vanity-grinder")]
#[clap(about = "GPU-accelerated Solana vanity address generator", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a performance benchmark
    Benchmark,
    
    /// Estimate time to find an address with pattern of specified length
    Estimate {
        /// Length of the pattern to search for
        pattern_length: u32,
        
        /// Whether the search is case sensitive (default: true)
        #[clap(long, default_value = "true")]
        case_sensitive: bool,
    },
    
    /// Generate a vanity address
    Generate {
        /// Pattern to search for
        pattern: String,
        
        /// Match pattern at the end of the address
        #[clap(long)]
        suffix: bool,
        
        /// Case-insensitive matching (faster)
        #[clap(long)]
        no_case_sensitive: bool,
        
        /// Maximum number of attempts before giving up
        #[clap(long)]
        max_attempts: Option<u64>,
    },
    
    /// Start the REST API server
    Serve {
        /// Host to bind to
        #[clap(long, default_value = "127.0.0.1")]
        host: String,
        
        /// Port to listen on
        #[clap(long, default_value = "7777")]
        port: u16,
        
        /// Allowed origins for CORS (comma-separated)
        #[clap(long, default_value = "http://localhost:3000,http://localhost:8080")]
        allowed_origins: String,
    },
}

async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    
    // Only run in legacy command mode if we have old-style arguments
    if args.len() >= 2 && !args[1].starts_with("-") && 
       !["benchmark", "estimate", "generate", "serve", "help"].contains(&args[1].to_lowercase().as_str()) {
        return run_legacy_mode(args);
    }
    
    // Parse command line arguments using Clap
    let cli = Cli::parse();
    
    match cli.command {
        Some(Commands::Benchmark) => {
            run_benchmark_command();
        },
        Some(Commands::Estimate { pattern_length, case_sensitive }) => {
            run_estimate_command(pattern_length, case_sensitive);
        },
        Some(Commands::Generate { pattern, suffix, no_case_sensitive, max_attempts }) => {
            run_generate_command(&pattern, suffix, !no_case_sensitive, max_attempts);
        },
        Some(Commands::Serve { host, port, allowed_origins }) => {
            let allowed_origins: Vec<String> = allowed_origins.split(',')
                .map(|s| s.trim().to_string())
                .collect();
            
            // Add the Command Server's IP
            let mut origins = allowed_origins.clone();
            origins.push("http://147.79.74.67".to_string());
            origins.push("https://147.79.74.67".to_string());
            
            println!("{}", "Starting API server...".green().bold());
            println!("Listening on {}:{}", host, port);
            println!("Allowed origins: {}", origins.join(", "));
            
            api::run_api_server(&host, port, origins).await?;
        },
        None => {
            print_banner();
            println!("Use --help for more information");
        }
    }
    
    Ok(())
}

/// Print welcome banner
fn print_banner() {
    let title = "Solana Vanity Address Grinder".bright_green().bold();
    println!("{}", title);
    println!("{}", "-----------------------------".green());
}

/// Run the benchmark command
fn run_benchmark_command() {
    println!("{}", "Initializing CUDA...".yellow());
    
    match cuda_helpers::CudaDevice::new() {
        Ok(device) => {
            println!("{}", device.get_info());
            println!("\n{}", "Finding optimal batch size for GPU benchmarking...".yellow());
            
            match cuda::vanity_generator::find_optimal_batch_size(&device) {
                Ok(optimal_batch_size) => {
                    println!("{}: {}", "Optimal batch size".green().bold(), format_number(optimal_batch_size as f64).cyan());
                    println!("\n{}", "Running CUDA benchmark with 5 iterations...".yellow());
                    
                    let mut samples = Vec::new();
                    for i in 0..5 {
                        match cuda_helpers::benchmark_batch_gpu(&device, optimal_batch_size) {
                            Ok(rate) => {
                                samples.push(rate);
                                let rate_str = format_number(rate).cyan();
                                println!("Iteration {}: {} {}", (i + 1).to_string().green(), rate_str, "keys/s".cyan());
                            },
                            Err(e) => {
                                eprintln!("{}: {}", "Error in benchmark iteration".red().bold(), e);
                                break;
                            }
                        }
                    }
                    
                    if !samples.is_empty() {
                        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
                        println!("\n{}", "Benchmark complete!".green().bold());
                        println!("{}: {} {}", "Average performance".yellow().bold(), 
                                 format_number(mean).cyan().bold(), "keys/second".cyan());
                        println!("{}: {}", "Optimal batch size".yellow(), format_number(optimal_batch_size as f64).cyan());
                    }
                },
                Err(e) => {
                    eprintln!("{}: {}", "Failed to find optimal batch size".red().bold(), e);
                }
            }
        },
        Err(e) => {
            eprintln!("{}: {}", "CUDA initialization failed".red().bold(), e);
            eprintln!("{}", "Make sure you have a compatible NVIDIA GPU and CUDA drivers installed.".yellow());
        }
    }
}

/// Run the estimate command
fn run_estimate_command(pattern_length: u32, case_sensitive: bool) {
    // Try to get GPU performance
    println!("{}", "Measuring GPU performance...".yellow());
    let gpu_rate = match cuda_helpers::CudaDevice::new() {
        Ok(device) => {
            match cuda::vanity_generator::find_optimal_batch_size(&device) {
                Ok(batch_size) => {
                    match cuda_helpers::benchmark_batch_gpu(&device, batch_size) {
                        Ok(rate) => {
                            // Print final GPU performance
                            println!("{}: {} {}", 
                                     "GPU performance".green().bold(), 
                                     format_number(rate).cyan().bold(), 
                                     "keypairs/second".cyan());
                            rate
                        },
                        Err(e) => {
                            eprintln!("{}: {}", "Error in benchmark".red().bold(), e);
                            1_000_000.0 // Default fallback
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}: {}", "Failed to find optimal batch size".red().bold(), e);
                    1_000_000.0 // Default fallback
                }
            }
        },
        Err(e) => {
            eprintln!("{}: {}", "CUDA initialization failed".red().bold(), e);
            println!("Using default performance estimate of {} keys/second", "1,000,000".yellow());
            1_000_000.0 // Default fallback
        }
    };
    
    // Use the measured rate for estimation
    let estimate = estimator::estimate_time(pattern_length, case_sensitive, gpu_rate);
    println!("\n{}", "Time Estimate:".green().bold());
    println!("{}", estimator::format_estimate(&estimate, &case_sensitive));
}

/// Run the generate command
fn run_generate_command(pattern: &str, is_suffix: bool, case_sensitive: bool, max_attempts: Option<u64>) {
    // Validate the pattern against base58 alphabet
    for c in pattern.chars() {
        if !"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(c) {
            eprintln!("{}: Invalid character '{}' in pattern. Only Base58 characters are allowed.", 
                      "Error".red().bold(), c);
            process::exit(1);
        }
    }
    
    // Initialize CUDA
    println!("{}", "Initializing CUDA...".yellow());
    match cuda_helpers::CudaDevice::new() {
        Ok(device) => {
            println!("{}", device.get_info());
            println!("\n{}", "Finding optimal batch size...".yellow());
            
            match cuda::vanity_generator::find_optimal_batch_size(&device) {
                Ok(batch_size) => {
                    println!("{}: {}", "Optimal batch size".green().bold(), 
                             format_number(batch_size as f64).cyan());
                    
                    // Prepare for search
                    let vanity_mode = if is_suffix {
                        VanityMode::Suffix
                    } else {
                        VanityMode::Prefix
                    };
                    
                    let case_info = if case_sensitive {
                        "case-sensitive".yellow()
                    } else {
                        "case-insensitive".green()
                    };
                    
                    let pattern_display = if pattern.len() > 20 {
                        format!("{}..", &pattern[0..20])
                    } else {
                        pattern.to_string()
                    };
                    
                    let mode_name = if is_suffix { "suffix" } else { "prefix" };
                    println!("\n{}", "Starting address search...".green().bold());
                    println!("Searching for {} {} {} ({}):", 
                             mode_name.cyan(),
                             pattern_display.bright_green().bold(),
                             "pattern".cyan(),
                             case_info);
                    
                    // Calculate expected search time
                    let pattern_length = pattern.len() as u32;
                    match cuda_helpers::benchmark_batch_gpu(&device, batch_size) {
                        Ok(rate) => {
                            let estimate = estimator::estimate_time(pattern_length, case_sensitive, rate);
                            println!("{}", "Expected search time: ".yellow().to_string() + 
                                     &estimate.expected_time.cyan().bold().to_string());
                            
                            // Start the search
                            let search_result = generate_vanity_address(
                                &device,
                                pattern,
                                vanity_mode,
                                case_sensitive,
                                batch_size,
                                max_attempts
                            );
                            
                            match search_result {
                                Ok(result) => {
                                    // We found a match!
                                    println!("\n{}", "🎉 Found a matching address! 🎉".green().bold());
                                    println!("{}: {}", "Address".blue().bold(), result.address.bright_green());
                                    println!("{}: {} {}", "Found in".blue().bold(), 
                                             format_number(result.attempts as f64).cyan(),
                                             "attempts".cyan());
                                    println!("{}: {:.2} seconds", "Time taken".blue().bold(), 
                                             result.duration.as_secs_f64());
                                    println!("{}: {} keys/second", "Average speed".blue().bold(), 
                                             format_number(result.attempts as f64 / result.duration.as_secs_f64()).cyan());
                                    
                                    // Save the keypair to a file
                                    let keypair_bytes = result.keypair.to_bytes();
                                    let base_path = Path::new("keys");
                                    if !base_path.exists() {
                                        fs::create_dir_all(base_path).unwrap_or_else(|_| {
                                            eprintln!("{}: Failed to create keys directory", "Warning".yellow().bold());
                                        });
                                    }
                                    
                                    // Create descriptive filename
                                    let mode_prefix = if is_suffix { "suffix" } else { "prefix" };
                                    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                                    let filename = format!("{}/{}_{}_{}_{}.json", 
                                                          base_path.display(), mode_prefix, pattern, timestamp, 
                                                          &result.address[0..8]);
                                    
                                    // Save keypair in JSON format
                                    let json = serde_json::to_string(&keypair_bytes.to_vec())
                                        .unwrap_or_else(|_| "[]".to_string());
                                    
                                    match fs::write(&filename, json) {
                                        Ok(_) => {
                                            println!("{}: {}", "Keypair saved to".green().bold(), filename);
                                        },
                                        Err(e) => {
                                            eprintln!("{}: Failed to save keypair: {}", "Error".red().bold(), e);
                                        }
                                    }
                                },
                                Err(e) => {
                                    eprintln!("{}: {}", "Error during search".red().bold(), e);
                                }
                            }
                        },
                        Err(e) => {
                            eprintln!("{}: {}", "Error in benchmark".red().bold(), e);
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}: {}", "Failed to find optimal batch size".red().bold(), e);
                }
            }
        },
        Err(e) => {
            eprintln!("{}: {}", "CUDA initialization failed".red().bold(), e);
            eprintln!("{}", "Make sure you have a compatible NVIDIA GPU and CUDA drivers installed.".yellow());
        }
    }
}

/// Run in legacy command mode for backward compatibility
fn run_legacy_mode(args: Vec<String>) -> std::io::Result<()> {
    match args[1].as_str() {
        "benchmark" => {
            run_benchmark_command();
        },
        "estimate" => {
            if args.len() < 3 {
                eprintln!("Usage: {} {} [case_sensitive=true]", args[0].cyan(), "estimate <pattern_length>".green());
                process::exit(1);
            }
            
            let pattern_length = match args[2].parse::<u32>() {
                Ok(n) => n,
                Err(_) => {
                    eprintln!("{}: pattern_length must be a positive integer", "Error".red().bold());
                    process::exit(1);
                }
            };
            
            let case_sensitive = if args.len() > 3 {
                match args[3].to_lowercase().as_str() {
                    "true" | "yes" | "1" => true,
                    "false" | "no" | "0" => false,
                    _ => {
                        eprintln!("{}: case_sensitive must be true/false", "Error".red().bold());
                        process::exit(1);
                    }
                }
            } else {
                true
            };
            
            run_estimate_command(pattern_length, case_sensitive);
        },
        "generate" => {
            if args.len() < 3 {
                eprintln!("Usage: {} {} <pattern> [options]", args[0].cyan(), "generate".green());
                eprintln!("Options:");
                eprintln!("  {} - Match pattern at the end of address", "--suffix".cyan());
                eprintln!("  {} - Case-insensitive matching (faster)", "--no-case-sensitive".cyan());
                process::exit(1);
            }
            
            // Parse command line options
            let mut pattern = String::new();
            let mut is_suffix = false;
            let mut case_sensitive = true;
            let mut max_attempts: Option<u64> = None;
            
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--suffix" => {
                        is_suffix = true;
                        i += 1;
                    },
                    "--no-case-sensitive" => {
                        case_sensitive = false;
                        i += 1;
                    },
                    "--max-attempts" => {
                        if i + 1 < args.len() {
                            match args[i + 1].parse::<u64>() {
                                Ok(n) => {
                                    max_attempts = Some(n);
                                    i += 2;
                                },
                                Err(_) => {
                                    eprintln!("{}: max-attempts must be a positive integer", "Error".red().bold());
                                    process::exit(1);
                                }
                            }
                        } else {
                            eprintln!("{}: --max-attempts requires a value", "Error".red().bold());
                            process::exit(1);
                        }
                    },
                    _ => {
                        if pattern.is_empty() {
                            pattern = args[i].clone();
                        } else {
                            eprintln!("{}: Unexpected argument '{}'", "Error".red().bold(), args[i]);
                            process::exit(1);
                        }
                        i += 1;
                    }
                }
            }
            
            if pattern.is_empty() {
                eprintln!("{}: Pattern is required", "Error".red().bold());
                process::exit(1);
            }
            
            run_generate_command(&pattern, is_suffix, case_sensitive, max_attempts);
        },
        _ => {
            // Try to interpret as a pattern length for backward compatibility
            if let Ok(pattern_length) = args[1].parse::<u32>() {
                let case_sensitive = if args.len() > 2 {
                    match args[2].to_lowercase().as_str() {
                        "true" | "yes" | "1" => true,
                        "false" | "no" | "0" => false,
                        _ => {
                            eprintln!("{}: case_sensitive must be true/false", "Error".red().bold());
                            process::exit(1);
                        }
                    }
                } else {
                    true
                };
                
                let gpu_rate = if args.len() > 3 {
                    match args[3].parse::<f64>() {
                        Ok(n) => n,
                        Err(_) => {
                            eprintln!("{}: gpu_rate must be a positive number", "Error".red().bold());
                            process::exit(1);
                        }
                    }
                } else {
                    1_000_000.0 // Default: 1M addresses per second
                };
                
                println!("{}: {} keys/second", "Note".yellow().bold(), format_number(gpu_rate).cyan());
                println!("      For more accurate estimates, use: {} {}", args[0].cyan(), "estimate <pattern_length>".green());
                
                let estimate = estimator::estimate_time(pattern_length, case_sensitive, gpu_rate);
                println!("{}", estimator::format_estimate(&estimate, &case_sensitive));
            } else {
                eprintln!("{}: Unknown command '{}'", "Error".red().bold(), args[1]);
                eprintln!("Run with no arguments to see available commands");
                process::exit(1);
            }
        }
    }
    
    Ok(())
}