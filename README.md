# Solana Vanity Address Grinder

A high-performance tool for generating Solana vanity addresses using GPU acceleration.

## Overview

This tool efficiently searches for Solana wallet addresses that match specified patterns by leveraging GPU processing power.

## Features

- GPU acceleration via CUDA/OpenCL
- Flexible pattern matching (prefix, suffix, contains, position-specific)
- Performance estimator to predict search time
- Direct keypair generation for maximum speed
- Secure storage of generated keypairs

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/vanity-grinder.git
cd vanity-grinder

# Build with CUDA support
cargo build --release --features cuda

# Or build with OpenCL support
cargo build --release --features opencl
```

## Usage

### Time Estimator

```bash
# Estimate time to find a 4-character prefix with case sensitivity
cargo run -- 4 true 1000000

# Estimate time to find a 6-character prefix without case sensitivity
cargo run -- 6 false 1000000
```

### Address Generation

```bash
# Generate address with prefix "DEGEN"
cargo run --release -- generate DEGEN

# Generate address with suffix "MOON"
cargo run --release -- generate --suffix MOON

# Generate case-insensitive address with prefix "btc" (faster)
cargo run --release -- generate btc --no-case-sensitive

# Run with limited attempts
cargo run --release -- generate DEGEN --max-attempts 10000000
```

## Performance

Performance depends on your GPU, but you can expect:

- Mid-range GPU: ~100,000+ keypairs/second
- High-end GPU: ~1,000,000+ keypairs/second

## Project Status

- [x] Time estimator
- [x] Core keypair generation
- [x] GPU acceleration
- [x] Pattern matching (prefix and suffix)
- [x] CLI interface
- [x] Secure storage (keypairs saved as JSON)

## License

This project is licensed under the MIT License - see the LICENSE file for details.