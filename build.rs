use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Tell cargo to look for CUDA in standard locations
    println!("cargo:rustc-link-search=native=/usr/local/cuda/lib64");
    println!("cargo:rustc-link-search=native=/usr/lib/x86_64-linux-gnu");
    println!("cargo:rustc-link-search=native=/usr/lib/aarch64-linux-gnu");
    
    // Link CUDA libraries
    println!("cargo:rustc-link-lib=dylib=cuda");
    println!("cargo:rustc-link-lib=dylib=cudart");
    println!("cargo:rustc-link-lib=dylib=curand");
    
    // Create CUDA sources directory if it doesn't exist
    std::fs::create_dir_all("src/cuda").unwrap_or_default();
    
    // Get output directory
    let out_dir = env::var("OUT_DIR").unwrap();
    let kernel_path = PathBuf::from(&out_dir).join("vanity_kernel.ptx");
    
    // Get CUDA version
    let output = Command::new("nvcc")
        .arg("--version")
        .output()
        .expect("Failed to execute nvcc --version");
    
    let version_str = String::from_utf8_lossy(&output.stdout);
    println!("CUDA Version: {}", version_str);
    
    // Compile the CUDA kernel to PTX with curand
    let status = Command::new("nvcc")
        .arg("--ptx")
        .arg("-arch=sm_60") // For compatibility with most CUDA devices
        .arg("-lcurand")
        .arg("-o")
        .arg(&kernel_path)
        .arg("src/cuda/vanity_kernel.cu")
        .status()
        .expect("Failed to compile CUDA kernel");
    
    if !status.success() {
        panic!("Failed to compile CUDA kernel");
    }
    
    // Tell Cargo that if the CUDA source changes, to rerun this build script
    println!("cargo:rerun-if-changed=src/cuda/vanity_kernel.cu");
    
    // Generate Rust file that includes the PTX code
    let ptx_content = std::fs::read_to_string(&kernel_path).expect("Failed to read PTX file");
    
    let rs_file = format!(
        "pub const PTX_SRC: &str = r###\"{}\"###;",
        ptx_content
    );
    
    let output_path = PathBuf::from(&out_dir).join("cuda_ptx.rs");
    std::fs::write(&output_path, rs_file).expect("Failed to write Rust PTX include file");
    
    println!("cargo:warning=Created PTX include at {:?}", output_path);
}