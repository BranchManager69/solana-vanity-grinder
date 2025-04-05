# Solana Vanity Address Grinder

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/BranchManager69/solana-vanity-grinder?style=social)](https://github.com/BranchManager69/solana-vanity-grinder/stargazers)
[![CUDA](https://img.shields.io/badge/CUDA-11.0%2B-76B900?logo=nvidia)](https://developer.nvidia.com/cuda-toolkit)
[![Solana](https://img.shields.io/badge/Solana-Compatible-14F195?logo=solana)](https://solana.com)
[![Rust](https://img.shields.io/badge/Made%20with-Rust-DEA584?logo=rust)](https://www.rust-lang.org)
[![Release](https://img.shields.io/github/v/release/BranchManager69/solana-vanity-grinder?include_prereleases&color=blue)](https://github.com/BranchManager69/solana-vanity-grinder/releases)
[![Build Status](https://img.shields.io/github/actions/workflow/status/BranchManager69/solana-vanity-grinder/rust.yml?branch=main&logo=github)](https://github.com/BranchManager69/solana-vanity-grinder/actions)

An ultra-high performance tool for generating Solana vanity addresses utilizing advanced CUDA-accelerated Ed25519 keypair generation.

## Technical Overview

This tool performs massively parallel GPU-based computation to generate and search for Solana wallet addresses matching user-specified patterns. Unlike CPU-based solutions that typically achieve 10-50K ops/sec, this implementation leverages the following advanced techniques:

- **CUDA Kernel Architecture**: Custom-designed PTX-optimized kernels with direct driver API integration
- **cuRAND Integration**: Hardware-accelerated randomness generation for ED25519 entropy
- **Batch Processing**: Dynamically optimized batch sizes (from 100K-10M) for maximum GPU utilization
- **Memory Management**: Zero-copy operations where possible with optimized host-device transfers
- **Base58 Pattern Matching**: GPU-accelerated case-sensitive and case-insensitive pattern matching
- **Ed25519 Keypair Generation**: Direct keypair derivation on GPU for maximum throughput

## Architectural Design

The system consists of three key components:

1. **Cuda Helpers Layer**: CUDA driver binding layer with memory management
2. **Kernel Implementation**: Pattern matching and keypair generation kernels
3. **Vanity Generator**: Pattern validation and search orchestration

### Memory Architecture

```
┌─────────────────┐      ┌──────────────────────────────────┐
│ Host (CPU)      │      │ Device (GPU)                     │
│                 │      │                                   │
│  ┌───────────┐  │      │  ┌───────────┐   ┌───────────┐   │
│  │ Pattern   ├──┼──────┼─►│ d_pattern │   │ cuRAND    │   │
│  │ Validation│  │      │  │           │   │ States    │   │
│  └───────────┘  │      │  └─────┬─────┘   └─────┬─────┘   │
│                 │      │        │               │         │
│  ┌───────────┐  │      │  ┌─────▼─────────────┐          │
│  │ Results   │◄─┼──────┼──┤ Parallel Pattern  │          │
│  │ Processing│  │      │  │ Matching          │◄─────┘   │
│  └───────────┘  │      │  └───────────────────┘          │
└─────────────────┘      └──────────────────────────────────┘
```

## CUDA Kernel Optimizations

Our CUDA kernels implement several critical optimizations:

- **Thread Coarsening**: Each thread processes multiple keypairs to improve instruction-level parallelism
- **Memory Coalescing**: Aligned memory access patterns for maximum throughput
- **Warp Divergence Minimization**: Carefully designed control flow to reduce branch inefficiency
- **Register Pressure Management**: Optimized register usage to maintain high occupancy
- **Atomic Result Reporting**: Lock-free result reporting using atomic operations

## Mathematical Probability Analysis

The difficulty for a prefix pattern of length `L` in Base58:

```
P(match) = (1/58)^L for case-sensitive
P(match) = (1/58)^L * (33/58)^L for case-insensitive
```

Expected number of attempts: `E(attempts) = 1/P(match)`

## Performance Metrics

| GPU               | Architecture | Keypairs/sec | 4-char Pattern ETA |
|-------------------|--------------|--------------|-------------------|
| RTX 3090          | Ampere       | ~1,200,000   | ~23 seconds       |
| RTX 2080 Ti       | Turing       | ~900,000     | ~32 seconds       |
| GTX 1080 Ti       | Pascal       | ~600,000     | ~48 seconds       |
| Tesla V100        | Volta        | ~1,500,000   | ~18 seconds       |
| A100              | Ampere       | ~2,500,000   | ~11 seconds       |

*Benchmarks measured on reference systems. Your performance may vary depending on system configuration and drivers.*

## Dynamic Batch Size Optimization

The tool automatically benchmarks your GPU to find the optimal batch size that maximizes throughput:

```
Batch Size         Relative Performance
100,000            ~60-70% of maximum
500,000            ~80-90% of maximum
1,000,000          ~90-95% of maximum
5,000,000          ~95-100% of maximum
10,000,000         ~98-100% of maximum (may fail on GPUs with limited memory)
```

## Installation Requirements

- CUDA Toolkit 11.0+ (10.0+ may work with reduced functionality)
- Rust 1.81.0+ with cargo (important for dependency compatibility)
- NVIDIA GPU with Compute Capability 6.0+ (Pascal architecture or newer)

### Standard Installation

```bash
# Clone the repository
git clone https://github.com/BranchManager69/solana-vanity-grinder.git
cd solana-vanity-grinder

# Build with optimizations
cargo build --release

# Verify CUDA device compatibility
./target/release/vanity-grinder benchmark
```

### Lambda Labs GPU Instance Setup

If you're using a Lambda Labs GPU instance or similar cloud GPU environment, the setup requires a few additional steps due to system Rust version constraints. We've created a simple setup process:

1. **First-time setup (just once):**

```bash
# Navigate to your persistent storage directory
cd /home/ubuntu/degenduel-gpu

# Run the provided setup script
./run-vanity.sh
```

This script handles:
- Setting up the necessary Rust 1.81.0 environment
- Building the project with all optimizations
- Displaying available commands

2. **Starting in future sessions:**

```bash
# From any directory, run:
/home/ubuntu/degenduel-gpu/run-vanity.sh

# This will:
# - Set up the Rust environment
# - Change to the vanity-grinder directory
# - Show available commands
```

The script creates symlinks in your persistent storage to ensure the correct Rust version is used each time, even if the Lambda Labs instance has an older system Rust version.

## Technical Usage

### Performance Benchmarking

```bash
# Measure raw keypair generation performance
cargo run --release -- benchmark
```

### Probabilistic Time Estimation

```bash
# Estimate search time for a pattern of specific length with mathematical model
cargo run --release -- estimate <pattern_length> [case_sensitive]

# Examples
cargo run --release -- estimate 4 true   # Case-sensitive 4-char pattern
cargo run --release -- estimate 6 false  # Case-insensitive 6-char pattern
```

### Vanity Address Generation

```bash
# Generate address with specific prefix (e.g., "DEGEN")
cargo run --release -- generate DEGEN

# Generate address with specific suffix (e.g., "MOON")
cargo run --release -- generate --suffix MOON

# Case-insensitive search (faster, matches both upper and lowercase)
cargo run --release -- generate btc --no-case-sensitive

# Limit maximum attempts (useful for scripting)
cargo run --release -- generate DEGEN --max-attempts 10000000
```

### REST API Server

The vanity-grinder also includes a full REST API for remote operation:

```bash
# Start the REST API server
cargo run --release -- serve --host 0.0.0.0 --port 7777

# Configure allowed CORS origins
cargo run --release -- serve --allowed-origins "http://localhost:3000,http://example.com"
```

#### API Endpoints

| Endpoint             | Method | Description                                |
|----------------------|--------|--------------------------------------------|
| `/health`            | GET    | Check API server status                    |
| `/jobs`              | GET    | List all vanity generation jobs            |
| `/jobs/{job_id}`     | GET    | Get status and result for a specific job   |
| `/jobs`              | POST   | Create a new vanity generation job         |
| `/jobs/{job_id}/cancel` | POST | Cancel a running job                      |

#### Example API Usage

Create a new job:

```bash
curl -X POST http://localhost:7777/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "pattern": "DEGEN",
    "is_suffix": false,
    "case_sensitive": true,
    "max_attempts": 100000000
  }'
```

Check job status:

```bash
curl http://localhost:7777/jobs/d290f1ee-6c54-4b01-90e6-d701748f0851
```

Example response for a completed job:
```json
{
  "id": "d290f1ee-6c54-4b01-90e6-d701748f0851",
  "status": "Completed",
  "request": {
    "pattern": "DEGEN",
    "is_suffix": false,
    "case_sensitive": true,
    "max_attempts": 100000000
  },
  "result": {
    "address": "DEGENkF9Pd6MX9MWdGxRMw89iG4Pw4vgJY5CTCTbY8or",
    "keypair_bytes": [236,18,32,...],
    "attempts": 3726489,
    "duration_ms": 15284,
    "rate_per_second": 243815
  },
  "created_at": "2025-04-05T08:12:34.123Z",
  "updated_at": "2025-04-05T08:13:12.456Z",
  "attempts": 3726489,
  "duration_ms": 15284
}
```

The `result` object contains everything you need:
- `address`: The Solana public address (as a string)
- `keypair_bytes`: Array of bytes representing the full keypair (can be used with Solana SDK)

To use the keypair in a Solana application or Command Server:

```javascript
// JavaScript example (Node.js with @solana/web3.js)
const { Keypair } = require('@solana/web3.js');

// Convert the keypair_bytes array from API response to Uint8Array
const keypairBytes = new Uint8Array(response.result.keypair_bytes);

// Create Solana keypair from bytes
const keypair = Keypair.fromSecretKey(keypairBytes);

// Now you can use this keypair for transactions
console.log('Address:', keypair.publicKey.toString());
```

Cancel a job:

```bash
curl -X POST http://localhost:7777/jobs/d290f1ee-6c54-4b01-90e6-d701748f0851/cancel
```

## Command Server Integration

The vanity-grinder REST API is designed to integrate seamlessly with the DegenDuel Command Server. This section provides detailed instructions for setting up this integration.

### Command Server Configuration

The vanity-grinder is pre-configured to accept requests from the Command Server at IP `147.79.74.67`. When you start the REST API server, this origin is automatically added to the allowed CORS list.

### Using Callbacks for Notification

Instead of polling for job status, the Command Server should provide a callback URL when creating a job:

```bash
curl -X POST http://localhost:7777/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "pattern": "DEGEN",
    "is_suffix": false,
    "case_sensitive": true,
    "callback_url": "http://147.79.74.67:8080/vanity-callback"
  }'
```

When the job completes or fails, the vanity-grinder will send a POST request to your callback URL with the complete job data, including:

- Job ID and status
- The pattern used
- The found address and keypair (if successful)
- Performance metrics

This allows your Command Server to be notified automatically when vanity addresses are found, eliminating the need for continuous polling.

### Command Server Implementation Example

Here's a simple Express.js-based Command Server implementation:

```javascript
const express = require('express');
const axios = require('axios');
const app = express();
app.use(express.json());

// Vanity Grinder API endpoint
const VANITY_GRINDER_URL = 'http://YOUR_VANITY_GRINDER_IP:7777';

// Endpoint to submit vanity address generation requests
app.post('/generate-vanity', async (req, res) => {
  const { pattern, is_suffix = false, case_sensitive = true } = req.body;
  
  try {
    // Submit job to vanity-grinder
    const response = await axios.post(`${VANITY_GRINDER_URL}/jobs`, {
      pattern,
      is_suffix,
      case_sensitive,
      callback_url: 'http://147.79.74.67:8080/vanity-callback' // Your Command Server callback URL
    });
    
    res.json({ job_id: response.data.job_id, status: 'submitted' });
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

// Callback endpoint to receive completed keypairs
app.post('/vanity-callback', (req, res) => {
  const job = req.body;
  
  if (job.status === 'Completed' && job.result) {
    // Process the found keypair
    const { address, keypair_bytes } = job.result;
    console.log(`Found address: ${address}`);
    
    // Store the keypair in your database
    // ...

    // Convert to Solana keypair if needed
    // const keypair = Keypair.fromSecretKey(new Uint8Array(keypair_bytes));
  }
  
  // Always acknowledge receipt
  res.status(200).send('OK');
});

// Listen on your Command Server IP
app.listen(8080, '0.0.0.0', () => {
  console.log('Command Server ready to dispatch vanity address jobs');
});
```

### Advanced Integration Features

- **Job Queuing**: The Command Server can queue multiple requests and track their status
- **Priority Control**: Implement priority levels for different types of vanity address generation
- **Rate Limiting**: Control the flow of requests to the vanity-grinder
- **Keypair Management**: Securely store and manage generated keypairs
- **Failure Handling**: Implement retry logic for failed jobs

The callback system ensures reliable notification even if the Command Server temporarily goes offline during processing.

#### API Endpoint Reference

**GET /health**
- Description: Check API server health
- Response: `{"status": "ok", "timestamp": "2025-04-05T12:34:56Z"}`

**GET /jobs**
- Description: List all vanity generation jobs
- Response: Array of job objects

**GET /jobs/{job_id}**
- Description: Get status and result for a specific job
- Response: Complete job object including result if completed

**POST /jobs**
- Description: Create a new vanity generation job
- Request Body:
  ```json
  {
    "pattern": "DEGEN",
    "is_suffix": false,          // optional, defaults to false (prefix)
    "case_sensitive": true,      // optional, defaults to true
    "max_attempts": 10000000,    // optional, defaults to unlimited
    "callback_url": "https://example.com/callback" // optional, for automatic notifications
  }
  ```
- Response: 
  ```json
  {
    "job_id": "d290f1ee-6c54-4b01-90e6-d701748f0851",
    "status": "queued",
    "notification": {
      "type": "callback",
      "message": "A POST request will be sent to your callback URL when the job completes"
    }
  }
  ```

**POST /jobs/{job_id}/cancel**
- Description: Cancel a running job
- Response: `{"status": "cancelled", "job_id": "d290f1ee-6c54-4b01-90e6-d701748f0851"}`

## Keypair Output Format

Generated keypairs are stored in JSON format compatible with Solana CLI tools:

```
[148,83,31,147,22,94,168,169,162,128,225,129,11,207,47,223,135,106,152,155,8,5,190,119,44,213,250,171,188,95,163,11]
```

## Architecture Decision Records

- **Direct CUDA Driver API**: Using low-level CUDA driver API rather than CUDA runtime API or frameworks like cuDNN/ArrayFire for maximum control over kernel execution and memory management
- **Ed25519 Implementation**: Simplified initial keypair generation on GPU with final validation on CPU to balance computation distribution
- **Base58 Encoding**: Implementing only the necessary subset of Base58 encoding on GPU to minimize computational overhead

## Project Status

- [x] Time estimator with advanced probability model
- [x] Core keypair generation with cuRAND acceleration 
- [x] GPU acceleration with dynamic batch sizing
- [x] Pattern matching (prefix, suffix) with case-sensitivity options
- [x] Comprehensive CLI interface with detailed reporting
- [x] Secure and compatible keypair storage (JSON format)
- [x] REST API for remote control via HTTP
- [x] Job queue management system

## Future Development Roadmap

- [ ] Multi-GPU support for horizontal scaling
- [ ] Position-specific pattern matching
- [ ] OpenCL backend for AMD/Intel GPUs
- [ ] CUDA Cooperative Groups for improved concurrency
- [ ] Unified Memory for simplified memory management
- [ ] Pattern complexity analyzer to estimate search time more accurately
- [ ] WebSocket notifications for job status changes
- [x] Support for external callback URLs after job completion

## Troubleshooting

### Rust Version Issues

If you encounter errors like:
```
error: package `native-tls v0.2.14` cannot be built because it requires rustc 1.80.0 or newer
```

Use our helper script which properly configures Rust 1.81.0:
```bash
/home/ubuntu/degenduel-gpu/run-vanity.sh
```

### CUDA Initialization Failures

If CUDA initialization fails, check:
- NVIDIA drivers are installed: `nvidia-smi`
- CUDA Toolkit is installed: `nvcc --version`
- Your GPU has Compute Capability 6.0+ (Pascal architecture or newer)

### API Server Connectivity Issues

If the Command Server can't connect to the vanity-grinder API:
- Ensure the API server is running on a publicly accessible interface: `--host 0.0.0.0`
- Verify port 7777 is open in any firewalls/security groups
- Check that the Command Server IP (147.79.74.67) is in the allowed CORS origins

### For Lambda Labs Environments

Lambda Labs instances may lose non-persistent data between sessions. Always:
- Store important keypairs in persistent storage (`/home/ubuntu/degenduel-gpu/`)
- Use the `run-vanity.sh` script when starting a new session
- Update the Command Server URL if your API server's IP changes

### Callback Issues

If callbacks aren't working:
- Make sure the Command Server is running and listening for callbacks
- Check for any network restrictions between the vanity-grinder and Command Server
- Verify the callback URL is properly formatted (http://147.79.74.67:8080/vanity-callback)

## License

This project is licensed under the MIT License - see the LICENSE file for details.