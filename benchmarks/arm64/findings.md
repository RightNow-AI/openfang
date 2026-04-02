# 📊 LLM Pipeline Benchmarking Findings (ARM64)

## 🏗️ Hardware Environment
- **Platform**: Oracle Cloud Compute (ARM64)
- **CPU**: Ampere Altra (Neoverse-N1) — **4 OCPUs / 4 Cores**
- **RAM**: 24 GB DDR4
- **Storage**: Balanced Block Volume

## 🤖 Model Performance (Liquid LFM2)
We tested the **Liquid Foundation Model 2** (8B and 2.6B) due to its extreme efficiency on ARM architecture.

| Model Variant | Input Size | Latency | Status |
| :--- | :--- | :--- | :--- |
| **LFM2-2.6B** | 100 Tokens | ~16.1s | Stable & Fast |
| **LFM2-8B** | 500 Tokens | **~24.4s** | ✅ **Optimal (Under 30s Wall)** |
| **LFM2-8B** | 700 Tokens | ~32.4s | ⚠️ Latency Wall Exceeded |
| **Llama-3.2-3B**| 100 Tokens | >120s | ❌ CPU Prefill Bottleneck |
| **Llama-3.1-8B**| 100 Tokens | >120s | ❌ CPU Prefill Bottleneck |

## ⚙️ Pipeline Parallelism & Threading
The goal was to maximize the 4 physical ARM cores.

| Configuration | Threads/Worker | Workers | Total Threads | Result |
| :--- | :--- | :--- | :--- | :--- |
| **Balanced** | 2 | 2 | 4 | **Peak Efficiency** (1:1 Thread-to-Core) |
| **Single Worker** | 4 | 1 | 4 | Baseline |
| **Over-subscribed** | 4 | 2 | 8 | **1273.2s total** (-5% vs Single) |

## 💡 Key Architectural Insights
1. **Thread-to-Core Affinity**: ARM Neoverse-N1 cores perform best when not over-subscribed. A 1:1 mapping of threads to cores prevents context switching and L2/L3 cache thrashing.
2. **Jumbo Chunking**: Using 2000-character chunks (~500 tokens) is the "sweet spot" for the 8B model to remain under the 30-second user-experience latency threshold.
3. **Memory Headroom**: Liquid-8B uses ~5GB of VRAM/RAM. On a 24GB instance, we can safely run up to 4 parallel workers, but CPU remains the primary bottleneck before RAM does.

## 🚀 Next Steps
- Integrating this pipeline into the OpenFang technical analysis workflow.
