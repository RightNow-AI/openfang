import time
from typing import TypedDict
from langchain_ollama import ChatOllama
from langgraph.graph import StateGraph, END

# ─────────────────────────────────────────
# 📊 BENCHMARK STATE
# ─────────────────────────────────────────
class BenchmarkState(TypedDict):
    token_count: int
    elapsed_time: float
    history: list[dict]
    stop: bool

# ─────────────────────────────────────────
# 🤖 MODEL SETUP
# ─────────────────────────────────────────
llm = ChatOllama(
    model="hf.co/LiquidAI/LFM2-8B-A1B-GGUF:Q4_K_M",
    num_thread=4,
    num_ctx=4096,
    temperature=0.0,
)

# ─────────────────────────────────────────
# ⚙️ NODES
# ─────────────────────────────────────────
def run_capacity_test(state: BenchmarkState):
    current_tokens = state["token_count"]
    
    # Approx 4 chars per token for simulation
    payload = "Data: " + ("token " * current_tokens)
    prompt = f"{payload}\n\nTask: Briefly summarize the data above in one sentence."
    
    print(f"🔄 Testing capacity: {current_tokens} tokens...", end="", flush=True)
    
    start_time = time.perf_counter()
    try:
        llm.invoke(prompt)
        end_time = time.perf_counter()
        elapsed = end_time - start_time
        print(f" ✅ {elapsed:.2f}s")
        
        # Stop if over 30s OR we've hit a reasonable limit for this test
        stop = elapsed > 30.0 or current_tokens > 4000
        return {
            "token_count": current_tokens + 200,
            "elapsed_time": elapsed,
            "history": state["history"] + [{"tokens": current_tokens, "time": elapsed}],
            "stop": stop
        }
    except Exception as e:
        print(f" ❌ Error: {e}")
        return {"stop": True}

def router(state: BenchmarkState):
    if state["stop"]:
        return "end"
    return "continue"

# ─────────────────────────────────────────
# 🕸️ GRAPH CONSTRUCTION
# ─────────────────────────────────────────
workflow = StateGraph(BenchmarkState)
workflow.add_node("test", run_capacity_test)
workflow.set_entry_point("test")

workflow.add_conditional_edges(
    "test",
    router,
    {
        "continue": "test",
        "end": END
    }
)

app = workflow.compile()

if __name__ == "__main__":
    print("\n🚀 Starting LangGraph Capacity Benchmark (Limit: 30s)")
    print("Model: sam860/LFM2:2.6b | Arch: ARM64\n")
    
    initial_state = {
        "token_count": 100,
        "elapsed_time": 0.0,
        "history": [],
        "stop": False
    }
    
    final_state = app.invoke(initial_state)
    
    print("\n" + "="*40)
    print("🏁 BENCHMARK RESULTS")
    print("="*40)
    
    last_success = None
    for entry in final_state["history"]:
        if entry["time"] <= 30.0:
            last_success = entry
        print(f"Tokens: {entry['tokens']:<5} | Latency: {entry['time']:.2f}s")
    
    if last_success:
        print("-" * 40)
        print(f"🏆 MAX CAPACITY (UNDER 30s): {last_success['tokens']} tokens")
    else:
        print("❌ Model failed to respond under 30s.")
