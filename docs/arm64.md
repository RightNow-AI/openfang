# OpenFang on ARM64 (aarch64)

This guide provides instructions for building and running OpenFang on ARM64 architectures (e.g., Oracle Cloud Neoverse-N1 and similar aarch64 Linux environments).

## Prerequisites

Before building from source, ensure you have the necessary Rust toolchain and native system dependencies installed.

### Native Dependencies (Ubuntu/Debian)
```bash
sudo apt update && sudo apt install -y \
  build-essential pkg-config libssl-dev \
  libglib2.0-dev libcairo2-dev libpango1.0-dev \
  libatk1.0-dev libgdk-pixbuf-2.0-dev libgtk-3-dev \
  libsoup-3.0-dev libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev libudev-dev
```

### Rust Toolchain
Requires Rust 1.75+:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

## Building from Source

```bash
git clone https://github.com/RightNow-AI/openfang.git
cd openfang
cargo build --release
```

The optimized binary will be located at `target/release/openfang`.

## Deployment Recommendations

### Headless Operation
For cloud instances, it is recommended to run the OpenFang kernel in headless mode:
```bash
./target/release/openfang init
./target/release/openfang start
```

### Performance & Hardware Scaling
Observations on ARM64 CPU-only environments (e.g., 4-core Neoverse-N1):

*   **Prompt Evaluation (Prefill) Latency**: On CPU-only ARM instances, high-context agentic workloads (large system prompts combined with tool schemas) can experience elevated latency during the prefill stage. Performance will vary significantly depending on the number of cores, clock speed, and memory bandwidth of the specific instance.
*   **Recommendation: Hybrid Mode**: For optimal and predictable performance on standard cloud CPUs, we recommend running the OpenFang **Kernel** locally and connecting to an external **OpenAI-compatible API** (e.g., Gemini, Groq, or OpenRouter) for the LLM reasoning component.

---
*Documentation provided by the OpenFang community.*
