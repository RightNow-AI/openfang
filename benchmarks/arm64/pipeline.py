import time
import os
from concurrent.futures import ThreadPoolExecutor, as_completed
from tqdm import tqdm
from langchain_ollama import ChatOllama

# ─────────────────────────────────────────
# 🧪 FINAL ARM64-OPTIMIZED MODEL SETUP
# ─────────────────────────────────────────
# Model: Liquid LFM2-8B (benchmarked at 500 tokens in under 30s)
# Architecture: ARM Neoverse-N1 (4 Cores)
# Memory: 24GB Total (5GB used by model)
llm = ChatOllama(
    model="hf.co/LiquidAI/LFM2-8B-A1B-GGUF:Q4_K_M",
    num_thread=4,
    num_ctx=4096,
    temperature=0.3,
)

# ─────────────────────────────────────────
# ✂️ CHUNKER WITH BATCH OVERLAP
# ─────────────────────────────────────────
def split_text(text: str, chunk_size: int = 2000, overlap: int = 200) -> list[str]:
    chunks = []
    start = 0
    while start < len(text):
        end = min(start + chunk_size, len(text))
        chunks.append(text[start:end])
        start += chunk_size - overlap
    return chunks

# ─────────────────────────────────────────
# 👷 WORKER WITH RETRY LOGIC
# ─────────────────────────────────────────
def process_chunk(chunk: str, retries: int = 2) -> str:
    prompt = f"""Summarize this technical document excerpt in 3-5 high-density bullet points. Focus on OpenFang's core architectural or security principles.

TEXT:
{chunk}

SUMMARY:"""
    for attempt in range(retries + 1):
        try:
            response = llm.invoke(prompt)
            return response.content.strip()
        except Exception as e:
            if attempt < retries:
                time.sleep(2)
            else:
                return f"[FAILED after {retries} retries: {e}]"

# ─────────────────────────────────────────
# 🧠 MAP-REDUCE AGGREGATOR
# ─────────────────────────────────────────
def aggregate(results: list[str]) -> str:
    combined = "\n".join(f"- {r}" for r in results)
    prompt = f"""You are a senior technical architect analyzing OpenFang for production deployment. Combine these technical excerpt summaries into a comprehensive High-Level Architecture Overview.

{combined}

FINAL TECHNICAL OVERVIEW:"""
    response = llm.invoke(prompt)
    return response.content.strip()

# ─────────────────────────────────────────
# 🚀 PARALLEL PIPELINE
# ─────────────────────────────────────────
def run_pipeline(text: str, max_workers: int = 2) -> str:
    chunks = split_text(text)
    print(f"\n💎 Final Analysis: OpenFang Documentation...")
    print(f"📄 Efficiently split into {len(chunks)} jumbo chunks (2k chars each).")
    print(f"🤖 Model: Liquid-8B (LFM2) | Workers: {max_workers} Parallel threads\n")

    results = [None] * len(chunks)

    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        future_to_index = {
            executor.submit(process_chunk, chunk): i
            for i, chunk in enumerate(chunks)
        }
        with tqdm(total=len(chunks), desc="⚙️ Analyzing") as pbar:
            for future in as_completed(future_to_index):
                idx = future_to_index[future]
                results[idx] = future.result()
                pbar.update(1)

    print("\n🏗️ Building Final Architecture Breakdown...\n")
    final = aggregate(results)
    return final

# ─────────────────────────────────────────
# ▶️ RUN ANALYSIS
# ─────────────────────────────────────────
if __name__ == "__main__":
    doc_path = "/home/ubuntu/openfang/docs/architecture.md"
    
    if not os.path.exists(doc_path):
        print(f"❌ Error: Could not find {doc_path}. Checking fallback...")
        doc_path = "/home/ubuntu/openfang/README.md"

    with open(doc_path, "r") as f:
        content = f.read()

    start = time.time()
    output = run_pipeline(content, max_workers=2)
    elapsed = time.time() - start

    print("\n" + "="*80)
    print("🚀 OPENFANG MASTER ARCHITECTURE OVERVIEW (LIQUID-8B / ARM64)")
    print("="*80)
    print(output)
    print(f"\n🏆 Final Analysis Time: {elapsed:.1f}s")
