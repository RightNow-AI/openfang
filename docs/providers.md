# LLM Providers Guide

OpenFang ships with a comprehensive model catalog covering **3 native LLM drivers**, **20 providers**, **51 builtin models**, and **23 aliases**. Every provider uses one of three battle-tested drivers: the native **Anthropic** driver, the native **Gemini** driver, or the universal **OpenAI-compatible** driver. This guide is the single source of truth for configuring, selecting, and managing LLM providers in OpenFang.

---

## Table of Contents

1. [Quick Setup](#quick-setup)
2. [Provider Reference](#provider-reference)
3. [Model Catalog](#model-catalog)
4. [Model Aliases](#model-aliases)
5. [Per-Agent Model Override](#per-agent-model-override)
6. [Model Routing](#model-routing)
7. [Cost Tracking](#cost-tracking)
8. [Fallback Providers](#fallback-providers)
9. [API Endpoints](#api-endpoints)
10. [Channel Commands](#channel-commands)

---

## Quick Setup

The fastest path from zero to running:

```bash
# Pick ONE provider — set its env var — done.
export GEMINI_API_KEY="your-key"        # Free tier available
# OR
export GROQ_API_KEY="your-key"          # Free tier available
# OR
export ANTHROPIC_API_KEY="your-key"
# OR
export OPENAI_API_KEY="your-key"
```

OpenFang auto-detects which providers have API keys configured at boot. Any model whose provider is authenticated becomes immediately available. Local providers (Ollama, vLLM, LM Studio) require no key at all.

For Gemini specifically, either `GEMINI_API_KEY` or `GOOGLE_API_KEY` will work.

---

## Provider Reference

### 1. Anthropic

| | |
|---|---|
| **Display Name** | Anthropic |
| **Driver** | Native Anthropic (Messages API) |
| **Env Var** | `ANTHROPIC_API_KEY` |
| **Base URL** | `https://api.anthropic.com` |
| **Key Required** | Yes |
| **Free Tier** | No |
| **Auth** | `x-api-key` header |
| **Models** | 3 |

**Available Models:**
- `claude-opus-4-20250514` (Frontier)
- `claude-sonnet-4-20250514` (Smart)
- `claude-haiku-4-5-20251001` (Fast)

**Setup:**
1. Sign up at [console.anthropic.com](https://console.anthropic.com)
2. Create an API key under Settings > API Keys
3. `export ANTHROPIC_API_KEY="sk-ant-..."`

---

### 2. OpenAI

| | |
|---|---|
| **Display Name** | OpenAI |
| **Driver** | OpenAI-compatible |
| **Env Var** | `OPENAI_API_KEY` |
| **Base URL** | `https://api.openai.com/v1` |
| **Key Required** | Yes |
| **Free Tier** | No |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 6 |

**Available Models:**
- `gpt-4.1` (Frontier)
- `gpt-4o` (Smart)
- `o3-mini` (Smart)
- `gpt-4.1-mini` (Balanced)
- `gpt-4o-mini` (Fast)
- `gpt-4.1-nano` (Fast)

**Setup:**
1. Sign up at [platform.openai.com](https://platform.openai.com)
2. Create an API key under API Keys
3. `export OPENAI_API_KEY="sk-..."`

---

### 3. Google Gemini

| | |
|---|---|
| **Display Name** | Google Gemini |
| **Driver** | Native Gemini (generateContent API) |
| **Env Var** | `GEMINI_API_KEY` (or `GOOGLE_API_KEY`) |
| **Base URL** | `https://generativelanguage.googleapis.com` |
| **Key Required** | Yes |
| **Free Tier** | Yes (generous free tier) |
| **Auth** | `x-goog-api-key` header |
| **Models** | 3 |

**Available Models:**
- `gemini-2.5-pro` (Frontier)
- `gemini-2.5-flash` (Smart)
- `gemini-2.0-flash` (Fast)

**Setup:**
1. Go to [aistudio.google.com](https://aistudio.google.com)
2. Get an API key (free tier included)
3. `export GEMINI_API_KEY="AIza..."` or `export GOOGLE_API_KEY="AIza..."`

**Notes:** The Gemini driver is a fully native implementation. It is not OpenAI-compatible. Model goes in the URL path, system prompt via `systemInstruction`, tools via `functionDeclarations`, streaming via `streamGenerateContent?alt=sse`.

---

### 4. DeepSeek

| | |
|---|---|
| **Display Name** | DeepSeek |
| **Driver** | OpenAI-compatible |
| **Env Var** | `DEEPSEEK_API_KEY` |
| **Base URL** | `https://api.deepseek.com/v1` |
| **Key Required** | Yes |
| **Free Tier** | No |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 2 |

**Available Models:**
- `deepseek-chat` (Smart) -- DeepSeek V3
- `deepseek-reasoner` (Smart) -- DeepSeek R1, no tool support

**Setup:**
1. Sign up at [platform.deepseek.com](https://platform.deepseek.com)
2. Create an API key
3. `export DEEPSEEK_API_KEY="sk-..."`

---

### 5. Groq

| | |
|---|---|
| **Display Name** | Groq |
| **Driver** | OpenAI-compatible |
| **Env Var** | `GROQ_API_KEY` |
| **Base URL** | `https://api.groq.com/openai/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (rate-limited) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 4 |

**Available Models:**
- `llama-3.3-70b-versatile` (Balanced)
- `mixtral-8x7b-32768` (Balanced)
- `llama-3.1-8b-instant` (Fast)
- `gemma2-9b-it` (Fast)

**Setup:**
1. Sign up at [console.groq.com](https://console.groq.com)
2. Create an API key
3. `export GROQ_API_KEY="gsk_..."`

**Notes:** Groq runs open-source models on custom LPU hardware. Extremely fast inference. Free tier has rate limits but is very usable.

---

### 6. OpenRouter

| | |
|---|---|
| **Display Name** | OpenRouter |
| **Driver** | OpenAI-compatible |
| **Env Var** | `OPENROUTER_API_KEY` |
| **Base URL** | `https://openrouter.ai/api/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (limited credits for some models) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 10 |

**Available Models:**
- `openrouter/google/gemini-2.5-flash` (Smart) -- cheap, fast, 1M context (default)
- `openrouter/anthropic/claude-sonnet-4` (Smart) -- strong reasoning + tools
- `openrouter/openai/gpt-4o` (Smart) -- GPT-4o via OpenRouter
- `openrouter/deepseek/deepseek-chat` (Smart) -- DeepSeek V3
- `openrouter/meta-llama/llama-3.3-70b-instruct` (Balanced) -- Llama 3.3 70B
- `openrouter/qwen/qwen-2.5-72b-instruct` (Balanced) -- Qwen 2.5 72B
- `openrouter/google/gemini-2.5-pro` (Frontier) -- Gemini 2.5 Pro
- `openrouter/mistralai/mistral-large-latest` (Smart) -- Mistral Large
- `openrouter/google/gemma-2-9b-it` (Fast) -- Gemma 2 9B, free
- `openrouter/deepseek/deepseek-r1` (Frontier) -- DeepSeek R1 reasoning

**Setup:**
1. Sign up at [openrouter.ai](https://openrouter.ai)
2. Create an API key under Keys
3. `export OPENROUTER_API_KEY="sk-or-..."`

**Notes:** OpenRouter is a unified gateway to 200+ models from many providers. Model IDs use the upstream format (e.g. `google/gemini-2.5-flash`). You can use any model from OpenRouter's catalog by specifying the full model path with the `openrouter/` prefix.

---

### 7. Mistral AI

| | |
|---|---|
| **Display Name** | Mistral AI |
| **Driver** | OpenAI-compatible |
| **Env Var** | `MISTRAL_API_KEY` |
| **Base URL** | `https://api.mistral.ai/v1` |
| **Key Required** | Yes |
| **Free Tier** | No |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 3 |

**Available Models:**
- `mistral-large-latest` (Smart)
- `codestral-latest` (Smart)
- `mistral-small-latest` (Fast)

**Setup:**
1. Sign up at [console.mistral.ai](https://console.mistral.ai)
2. Create an API key
3. `export MISTRAL_API_KEY="..."`

---

### 8. Together AI

| | |
|---|---|
| **Display Name** | Together AI |
| **Driver** | OpenAI-compatible |
| **Env Var** | `TOGETHER_API_KEY` |
| **Base URL** | `https://api.together.xyz/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (limited credits on signup) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 3 |

**Available Models:**
- `meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo` (Frontier)
- `Qwen/Qwen2.5-72B-Instruct-Turbo` (Smart)
- `mistralai/Mixtral-8x22B-Instruct-v0.1` (Balanced)

**Setup:**
1. Sign up at [api.together.ai](https://api.together.ai)
2. Create an API key
3. `export TOGETHER_API_KEY="..."`

---

### 9. Fireworks AI

| | |
|---|---|
| **Display Name** | Fireworks AI |
| **Driver** | OpenAI-compatible |
| **Env Var** | `FIREWORKS_API_KEY` |
| **Base URL** | `https://api.fireworks.ai/inference/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (limited credits on signup) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 2 |

**Available Models:**
- `accounts/fireworks/models/llama-v3p1-405b-instruct` (Frontier)
- `accounts/fireworks/models/mixtral-8x22b-instruct` (Balanced)

**Setup:**
1. Sign up at [fireworks.ai](https://fireworks.ai)
2. Create an API key
3. `export FIREWORKS_API_KEY="..."`

---

### 10. Ollama

| | |
|---|---|
| **Display Name** | Ollama |
| **Driver** | OpenAI-compatible |
| **Env Var** | `OLLAMA_API_KEY` (not required) |
| **Base URL** | `http://localhost:11434/v1` |
| **Key Required** | **No** |
| **Free Tier** | Free (local) |
| **Auth** | None (local) |
| **Models** | 3 builtin + auto-discovered |

**Available Models (builtin):**
- `llama3.2` (Local)
- `mistral:latest` (Local)
- `phi3` (Local)

**Setup:**
1. Install Ollama from [ollama.com](https://ollama.com)
2. Pull a model: `ollama pull llama3.2`
3. Start the server: `ollama serve`
4. No env var needed -- Ollama is always available

**Notes:** OpenFang auto-discovers models from a running Ollama instance and merges them into the catalog with `Local` tier and zero cost. Any model you pull becomes usable immediately.

---

### 11. vLLM

| | |
|---|---|
| **Display Name** | vLLM |
| **Driver** | OpenAI-compatible |
| **Env Var** | `VLLM_API_KEY` (not required) |
| **Base URL** | `http://localhost:8000/v1` |
| **Key Required** | **No** |
| **Free Tier** | Free (self-hosted) |
| **Auth** | None (local) |
| **Models** | 1 builtin + auto-discovered |

**Available Models (builtin):**
- `vllm-local` (Local)

**Setup:**
1. Install vLLM: `pip install vllm`
2. Start the server: `python -m vllm.entrypoints.openai.api_server --model <model-name>`
3. No env var needed

---

### 12. LM Studio

| | |
|---|---|
| **Display Name** | LM Studio |
| **Driver** | OpenAI-compatible |
| **Env Var** | `LMSTUDIO_API_KEY` (not required) |
| **Base URL** | `http://localhost:1234/v1` |
| **Key Required** | **No** |
| **Free Tier** | Free (local) |
| **Auth** | None (local) |
| **Models** | 1 builtin + auto-discovered |

**Available Models (builtin):**
- `lmstudio-local` (Local)

**Setup:**
1. Download LM Studio from [lmstudio.ai](https://lmstudio.ai)
2. Download a model from the built-in model browser
3. Start the local server from the "Local Server" tab
4. No env var needed

---

### 13. Perplexity AI

| | |
|---|---|
| **Display Name** | Perplexity AI |
| **Driver** | OpenAI-compatible |
| **Env Var** | `PERPLEXITY_API_KEY` |
| **Base URL** | `https://api.perplexity.ai` |
| **Key Required** | Yes |
| **Free Tier** | No |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 2 |

**Available Models:**
- `sonar-pro` (Smart) -- online search-augmented
- `sonar` (Balanced) -- online search-augmented

**Setup:**
1. Sign up at [perplexity.ai](https://www.perplexity.ai)
2. Go to API settings and generate a key
3. `export PERPLEXITY_API_KEY="pplx-..."`

**Notes:** Perplexity models have built-in web search. They do not support tool use.

---

### 14. Cohere

| | |
|---|---|
| **Display Name** | Cohere |
| **Driver** | OpenAI-compatible |
| **Env Var** | `COHERE_API_KEY` |
| **Base URL** | `https://api.cohere.com/v2` |
| **Key Required** | Yes |
| **Free Tier** | Yes (rate-limited trial) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 2 |

**Available Models:**
- `command-r-plus` (Smart)
- `command-r` (Balanced)

**Setup:**
1. Sign up at [dashboard.cohere.com](https://dashboard.cohere.com)
2. Create an API key
3. `export COHERE_API_KEY="..."`

---

### 15. AI21 Labs

| | |
|---|---|
| **Display Name** | AI21 Labs |
| **Driver** | OpenAI-compatible |
| **Env Var** | `AI21_API_KEY` |
| **Base URL** | `https://api.ai21.com/studio/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (limited credits) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 1 |

**Available Models:**
- `jamba-1.5-large` (Smart)

**Setup:**
1. Sign up at [studio.ai21.com](https://studio.ai21.com)
2. Create an API key
3. `export AI21_API_KEY="..."`

---

### 16. Cerebras

| | |
|---|---|
| **Display Name** | Cerebras |
| **Driver** | OpenAI-compatible |
| **Env Var** | `CEREBRAS_API_KEY` |
| **Base URL** | `https://api.cerebras.ai/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (generous free tier) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 2 |

**Available Models:**
- `cerebras/llama3.3-70b` (Balanced)
- `cerebras/llama3.1-8b` (Fast)

**Setup:**
1. Sign up at [cloud.cerebras.ai](https://cloud.cerebras.ai)
2. Create an API key
3. `export CEREBRAS_API_KEY="..."`

**Notes:** Cerebras runs inference on wafer-scale chips. Ultra-fast and ultra-cheap ($0.06/M tokens for both input and output on the 70B model).

---

### 17. SambaNova

| | |
|---|---|
| **Display Name** | SambaNova |
| **Driver** | OpenAI-compatible |
| **Env Var** | `SAMBANOVA_API_KEY` |
| **Base URL** | `https://api.sambanova.ai/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (limited credits) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 1 |

**Available Models:**
- `sambanova/llama-3.3-70b` (Balanced)

**Setup:**
1. Sign up at [cloud.sambanova.ai](https://cloud.sambanova.ai)
2. Create an API key
3. `export SAMBANOVA_API_KEY="..."`

---

### 18. Hugging Face

| | |
|---|---|
| **Display Name** | Hugging Face |
| **Driver** | OpenAI-compatible |
| **Env Var** | `HF_API_KEY` |
| **Base URL** | `https://api-inference.huggingface.co/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (rate-limited) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 1 |

**Available Models:**
- `hf/meta-llama/Llama-3.3-70B-Instruct` (Balanced)

**Setup:**
1. Sign up at [huggingface.co](https://huggingface.co)
2. Create a token under Settings > Access Tokens
3. `export HF_API_KEY="hf_..."`

---

### 19. xAI

| | |
|---|---|
| **Display Name** | xAI |
| **Driver** | OpenAI-compatible |
| **Env Var** | `XAI_API_KEY` |
| **Base URL** | `https://api.x.ai/v1` |
| **Key Required** | Yes |
| **Free Tier** | Yes (limited free credits) |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 2 |

**Available Models:**
- `grok-2` (Smart) -- supports vision
- `grok-2-mini` (Fast)

**Setup:**
1. Sign up at [console.x.ai](https://console.x.ai)
2. Create an API key
3. `export XAI_API_KEY="xai-..."`

---

### 20. Replicate

| | |
|---|---|
| **Display Name** | Replicate |
| **Driver** | OpenAI-compatible |
| **Env Var** | `REPLICATE_API_TOKEN` |
| **Base URL** | `https://api.replicate.com/v1` |
| **Key Required** | Yes |
| **Free Tier** | No |
| **Auth** | `Authorization: Bearer` header |
| **Models** | 1 |

**Available Models:**
- `replicate/meta-llama-3.3-70b-instruct` (Balanced)

**Setup:**
1. Sign up at [replicate.com](https://replicate.com)
2. Go to Account > API Tokens
3. `export REPLICATE_API_TOKEN="r8_..."`

---

## Model Catalog

The complete catalog of all 51 builtin models, sorted by provider. Pricing is per million tokens.

| # | Model ID | Display Name | Provider | Tier | Context Window | Max Output | Input $/M | Output $/M | Tools | Vision |
|---|----------|-------------|----------|------|---------------|------------|-----------|------------|-------|--------|
| 1 | `claude-opus-4-20250514` | Claude Opus 4 | anthropic | Frontier | 200,000 | 32,000 | $15.00 | $75.00 | Yes | Yes |
| 2 | `claude-sonnet-4-20250514` | Claude Sonnet 4 | anthropic | Smart | 200,000 | 64,000 | $3.00 | $15.00 | Yes | Yes |
| 3 | `claude-haiku-4-5-20251001` | Claude Haiku 4.5 | anthropic | Fast | 200,000 | 8,192 | $0.25 | $1.25 | Yes | Yes |
| 4 | `gpt-4.1` | GPT-4.1 | openai | Frontier | 1,047,576 | 32,768 | $2.00 | $8.00 | Yes | Yes |
| 5 | `gpt-4o` | GPT-4o | openai | Smart | 128,000 | 16,384 | $2.50 | $10.00 | Yes | Yes |
| 6 | `o3-mini` | o3-mini | openai | Smart | 200,000 | 100,000 | $1.10 | $4.40 | Yes | No |
| 7 | `gpt-4.1-mini` | GPT-4.1 Mini | openai | Balanced | 1,047,576 | 32,768 | $0.40 | $1.60 | Yes | Yes |
| 8 | `gpt-4o-mini` | GPT-4o Mini | openai | Fast | 128,000 | 16,384 | $0.15 | $0.60 | Yes | Yes |
| 9 | `gpt-4.1-nano` | GPT-4.1 Nano | openai | Fast | 1,047,576 | 32,768 | $0.10 | $0.40 | Yes | No |
| 10 | `gemini-2.5-pro` | Gemini 2.5 Pro | gemini | Frontier | 1,048,576 | 65,536 | $1.25 | $10.00 | Yes | Yes |
| 11 | `gemini-2.5-flash` | Gemini 2.5 Flash | gemini | Smart | 1,048,576 | 65,536 | $0.15 | $0.60 | Yes | Yes |
| 12 | `gemini-2.0-flash` | Gemini 2.0 Flash | gemini | Fast | 1,048,576 | 8,192 | $0.10 | $0.40 | Yes | Yes |
| 13 | `deepseek-chat` | DeepSeek V3 | deepseek | Smart | 64,000 | 8,192 | $0.27 | $1.10 | Yes | No |
| 14 | `deepseek-reasoner` | DeepSeek R1 | deepseek | Smart | 64,000 | 8,192 | $0.55 | $2.19 | No | No |
| 15 | `llama-3.3-70b-versatile` | Llama 3.3 70B | groq | Balanced | 128,000 | 32,768 | $0.059 | $0.079 | Yes | No |
| 16 | `mixtral-8x7b-32768` | Mixtral 8x7B | groq | Balanced | 32,768 | 4,096 | $0.024 | $0.024 | Yes | No |
| 17 | `llama-3.1-8b-instant` | Llama 3.1 8B | groq | Fast | 128,000 | 8,192 | $0.05 | $0.08 | Yes | No |
| 18 | `gemma2-9b-it` | Gemma 2 9B | groq | Fast | 8,192 | 4,096 | $0.02 | $0.02 | No | No |
| 19 | `openrouter/google/gemini-2.5-flash` | Gemini 2.5 Flash (OpenRouter) | openrouter | Smart | 1,048,576 | 65,536 | $0.15 | $0.60 | Yes | Yes |
| 20 | `openrouter/anthropic/claude-sonnet-4` | Claude Sonnet 4 (OpenRouter) | openrouter | Smart | 200,000 | 64,000 | $3.00 | $15.00 | Yes | Yes |
| 21 | `openrouter/openai/gpt-4o` | GPT-4o (OpenRouter) | openrouter | Smart | 128,000 | 16,384 | $2.50 | $10.00 | Yes | Yes |
| 22 | `openrouter/deepseek/deepseek-chat` | DeepSeek V3 (OpenRouter) | openrouter | Smart | 128,000 | 32,768 | $0.14 | $0.28 | Yes | No |
| 23 | `openrouter/meta-llama/llama-3.3-70b-instruct` | Llama 3.3 70B (OpenRouter) | openrouter | Balanced | 128,000 | 32,768 | $0.39 | $0.39 | Yes | No |
| 24 | `openrouter/qwen/qwen-2.5-72b-instruct` | Qwen 2.5 72B (OpenRouter) | openrouter | Balanced | 128,000 | 32,768 | $0.36 | $0.36 | Yes | No |
| 25 | `openrouter/google/gemini-2.5-pro` | Gemini 2.5 Pro (OpenRouter) | openrouter | Frontier | 1,048,576 | 65,536 | $1.25 | $10.00 | Yes | Yes |
| 26 | `openrouter/mistralai/mistral-large-latest` | Mistral Large (OpenRouter) | openrouter | Smart | 128,000 | 8,192 | $2.00 | $6.00 | Yes | No |
| 27 | `openrouter/google/gemma-2-9b-it` | Gemma 2 9B (OpenRouter) | openrouter | Fast | 8,192 | 4,096 | $0.00 | $0.00 | No | No |
| 28 | `openrouter/deepseek/deepseek-r1` | DeepSeek R1 (OpenRouter) | openrouter | Frontier | 128,000 | 32,768 | $0.55 | $2.19 | No | No |
| 29 | `mistral-large-latest` | Mistral Large | mistral | Smart | 128,000 | 8,192 | $2.00 | $6.00 | Yes | No |
| 30 | `codestral-latest` | Codestral | mistral | Smart | 32,000 | 8,192 | $0.30 | $0.90 | Yes | No |
| 31 | `mistral-small-latest` | Mistral Small | mistral | Fast | 128,000 | 8,192 | $0.10 | $0.30 | Yes | No |
| 32 | `meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo` | Llama 3.1 405B (Together) | together | Frontier | 130,000 | 4,096 | $3.50 | $3.50 | Yes | No |
| 33 | `Qwen/Qwen2.5-72B-Instruct-Turbo` | Qwen 2.5 72B (Together) | together | Smart | 32,768 | 4,096 | $0.20 | $0.60 | Yes | No |
| 34 | `mistralai/Mixtral-8x22B-Instruct-v0.1` | Mixtral 8x22B (Together) | together | Balanced | 65,536 | 4,096 | $0.60 | $0.60 | Yes | No |
| 35 | `accounts/fireworks/models/llama-v3p1-405b-instruct` | Llama 3.1 405B (Fireworks) | fireworks | Frontier | 131,072 | 16,384 | $3.00 | $3.00 | Yes | No |
| 36 | `accounts/fireworks/models/mixtral-8x22b-instruct` | Mixtral 8x22B (Fireworks) | fireworks | Balanced | 65,536 | 4,096 | $0.90 | $0.90 | Yes | No |
| 37 | `llama3.2` | Llama 3.2 (Ollama) | ollama | Local | 128,000 | 4,096 | $0.00 | $0.00 | Yes | No |
| 38 | `mistral:latest` | Mistral (Ollama) | ollama | Local | 32,768 | 4,096 | $0.00 | $0.00 | Yes | No |
| 39 | `phi3` | Phi-3 (Ollama) | ollama | Local | 128,000 | 4,096 | $0.00 | $0.00 | No | No |
| 40 | `vllm-local` | vLLM Local Model | vllm | Local | 32,768 | 4,096 | $0.00 | $0.00 | Yes | No |
| 41 | `lmstudio-local` | LM Studio Local Model | lmstudio | Local | 32,768 | 4,096 | $0.00 | $0.00 | Yes | No |
| 42 | `sonar-pro` | Sonar Pro | perplexity | Smart | 200,000 | 8,192 | $3.00 | $15.00 | No | No |
| 43 | `sonar` | Sonar | perplexity | Balanced | 128,000 | 8,192 | $1.00 | $5.00 | No | No |
| 44 | `command-r-plus` | Command R+ | cohere | Smart | 128,000 | 4,096 | $2.50 | $10.00 | Yes | No |
| 45 | `command-r` | Command R | cohere | Balanced | 128,000 | 4,096 | $0.15 | $0.60 | Yes | No |
| 46 | `jamba-1.5-large` | Jamba 1.5 Large | ai21 | Smart | 256,000 | 4,096 | $2.00 | $8.00 | Yes | No |
| 47 | `cerebras/llama3.3-70b` | Llama 3.3 70B (Cerebras) | cerebras | Balanced | 128,000 | 8,192 | $0.06 | $0.06 | Yes | No |
| 48 | `cerebras/llama3.1-8b` | Llama 3.1 8B (Cerebras) | cerebras | Fast | 128,000 | 8,192 | $0.01 | $0.01 | Yes | No |
| 49 | `sambanova/llama-3.3-70b` | Llama 3.3 70B (SambaNova) | sambanova | Balanced | 128,000 | 8,192 | $0.06 | $0.06 | Yes | No |
| 50 | `grok-2` | Grok 2 | xai | Smart | 131,072 | 32,768 | $2.00 | $10.00 | Yes | Yes |
| 51 | `grok-2-mini` | Grok 2 Mini | xai | Fast | 131,072 | 32,768 | $0.30 | $0.50 | Yes | No |
| 52 | `hf/meta-llama/Llama-3.3-70B-Instruct` | Llama 3.3 70B (HF) | huggingface | Balanced | 128,000 | 4,096 | $0.30 | $0.30 | No | No |
| 53 | `replicate/meta-llama-3.3-70b-instruct` | Llama 3.3 70B (Replicate) | replicate | Balanced | 128,000 | 4,096 | $0.40 | $0.40 | No | No |

**Model Tiers:**

| Tier | Description | Typical Use |
|------|------------|------------|
| **Frontier** | Most capable, highest cost | Orchestration, architecture, security audits |
| **Smart** | Strong reasoning, moderate cost | Coding, code review, research, analysis |
| **Balanced** | Good cost/quality tradeoff | Planning, writing, DevOps, day-to-day tasks |
| **Fast** | Cheapest cloud inference | Ops, translation, simple Q&A, health checks |
| **Local** | Self-hosted, zero cost | Privacy-first, offline, development |

**Notes:**
- Local providers (Ollama, vLLM, LM Studio) auto-discover models at runtime. Any model you download and serve will be merged into the catalog with `Local` tier and zero cost.
- The 46 entries above are the builtin models. The total of 51 referenced in the catalog includes runtime auto-discovered models that vary per installation.

---

## Model Aliases

All 23 aliases resolve to canonical model IDs. Aliases are case-insensitive.

| Alias | Resolves To |
|-------|------------|
| `sonnet` | `claude-sonnet-4-20250514` |
| `claude-sonnet` | `claude-sonnet-4-20250514` |
| `haiku` | `claude-haiku-4-5-20251001` |
| `claude-haiku` | `claude-haiku-4-5-20251001` |
| `opus` | `claude-opus-4-20250514` |
| `claude-opus` | `claude-opus-4-20250514` |
| `gpt4` | `gpt-4o` |
| `gpt4o` | `gpt-4o` |
| `gpt4-mini` | `gpt-4o-mini` |
| `flash` | `gemini-2.5-flash` |
| `gemini-flash` | `gemini-2.5-flash` |
| `gemini-pro` | `gemini-2.5-pro` |
| `deepseek` | `deepseek-chat` |
| `llama` | `llama-3.3-70b-versatile` |
| `llama-70b` | `llama-3.3-70b-versatile` |
| `mixtral` | `mixtral-8x7b-32768` |
| `mistral` | `mistral-large-latest` |
| `codestral` | `codestral-latest` |
| `grok` | `grok-2` |
| `grok-mini` | `grok-2-mini` |
| `sonar` | `sonar-pro` |
| `jamba` | `jamba-1.5-large` |
| `command-r` | `command-r-plus` |

You can use aliases anywhere a model ID is accepted: in config files, REST API calls, chat commands, and the model routing configuration.

---

## Per-Agent Model Override

Each agent in your `config.toml` can specify its own model, overriding the global default:

```toml
# Global default model
[agents.defaults]
model = "claude-sonnet-4-20250514"

# Per-agent override: use an alias or full model ID
[[agents]]
name = "orchestrator"
model = "opus"                      # alias for claude-opus-4-20250514

[[agents]]
name = "ops"
model = "llama-3.3-70b-versatile"   # cheap Groq model for simple ops

[[agents]]
name = "coder"
model = "gemini-2.5-flash"          # fast + cheap + 1M context

[[agents]]
name = "researcher"
model = "sonar-pro"                 # Perplexity with built-in web search

# You can also pin a model in the agent manifest TOML
[[agents]]
name = "production-bot"
pinned_model = "claude-sonnet-4-20250514"  # never auto-routed
```

When `pinned_model` is set on an agent manifest, that agent always uses the specified model regardless of routing configuration. This is used in **Stabilisation mode** (`KernelMode::Stable`) where the model is frozen for production reliability.

---

## Model Routing

OpenFang can automatically select the cheapest model capable of handling each query. This is configured per-agent via `ModelRoutingConfig`.

### How It Works

1. The **ModelRouter** scores each incoming `CompletionRequest` based on heuristics
2. The score maps to a **TaskComplexity** tier: `Simple`, `Medium`, or `Complex`
3. Each tier has a pre-configured model

### Scoring Heuristics

| Signal | Weight | Logic |
|--------|--------|-------|
| Total message length | 1 point per ~4 chars | Rough token proxy |
| Tool availability | +20 per tool defined | Tools imply multi-step work |
| Code markers | +30 per marker found | Backticks, `fn`, `def`, `class`, `import`, `function`, `async`, `await`, `struct`, `impl`, `return` |
| Conversation depth | +15 per message > 10 | Deep context = harder reasoning |
| System prompt length | +1 per 10 chars > 500 | Long system prompts imply complex tasks |

### Thresholds

| Complexity | Score Range | Default Model |
|-----------|-------------|---------------|
| Simple | score < 100 | `claude-haiku-4-5-20251001` |
| Medium | 100 <= score < 500 | `claude-sonnet-4-20250514` |
| Complex | score >= 500 | `claude-sonnet-4-20250514` |

### Configuration

```toml
# In agent manifest or config.toml
[routing]
simple_model = "claude-haiku-4-5-20251001"
medium_model = "gemini-2.5-flash"
complex_model = "claude-sonnet-4-20250514"
simple_threshold = 100
complex_threshold = 500
```

The router also integrates with the model catalog:
- **`validate_models()`** checks that all configured model IDs exist in the catalog
- **`resolve_aliases()`** expands aliases to canonical IDs (e.g., `"sonnet"` becomes `"claude-sonnet-4-20250514"`)

---

## Cost Tracking

OpenFang tracks the cost of every LLM call and can enforce per-agent spending quotas.

### Per-Response Cost Estimation

After each LLM call, cost is calculated as:

```
cost = (input_tokens / 1,000,000) * input_rate + (output_tokens / 1,000,000) * output_rate
```

The `MeteringEngine` first checks the **model catalog** for exact pricing. If the model is not found, it falls back to a pattern-matching heuristic.

### Cost Rates (per million tokens)

| Model Pattern | Input $/M | Output $/M |
|--------------|-----------|------------|
| `*haiku*` | $0.25 | $1.25 |
| `*sonnet*` | $3.00 | $15.00 |
| `*opus*` | $15.00 | $75.00 |
| `gpt-4o-mini` | $0.15 | $0.60 |
| `gpt-4o` | $2.50 | $10.00 |
| `gpt-4.1-nano` | $0.10 | $0.40 |
| `gpt-4.1-mini` | $0.40 | $1.60 |
| `gpt-4.1` | $2.00 | $8.00 |
| `o3-mini` | $1.10 | $4.40 |
| `gemini-2.5-pro` | $1.25 | $10.00 |
| `gemini-2.5-flash` | $0.15 | $0.60 |
| `gemini-2.0-flash` | $0.10 | $0.40 |
| `deepseek-reasoner` / `deepseek-r1` | $0.55 | $2.19 |
| `*deepseek*` | $0.27 | $1.10 |
| `*cerebras*` | $0.06 | $0.06 |
| `*sambanova*` | $0.06 | $0.06 |
| `*replicate*` | $0.40 | $0.40 |
| `*llama*` / `*mixtral*` | $0.05 | $0.10 |
| `*qwen*` | $0.20 | $0.60 |
| `mistral-large*` | $2.00 | $6.00 |
| `*mistral*` (other) | $0.10 | $0.30 |
| `command-r-plus` | $2.50 | $10.00 |
| `command-r` | $0.15 | $0.60 |
| `sonar-pro` | $3.00 | $15.00 |
| `*sonar*` (other) | $1.00 | $5.00 |
| `grok-2-mini` / `grok-mini` | $0.30 | $0.50 |
| `*grok*` (other) | $2.00 | $10.00 |
| `*jamba*` | $2.00 | $8.00 |
| Default (unknown) | $1.00 | $3.00 |

### Quota Enforcement

Quotas are checked on every LLM call. If the agent exceeds its hourly limit, the call is rejected with a `QuotaExceeded` error.

```toml
# Per-agent quota in config.toml
[[agents]]
name = "chatbot"
[agents.resources]
max_cost_per_hour_usd = 5.00   # cap at $5/hour
```

The usage footer (when enabled) appends cost information to each response:

```
> Cost: $0.0042 | Tokens: 1,200 in / 340 out | Model: claude-sonnet-4-20250514
```

---

## Fallback Providers

The `FallbackDriver` wraps multiple LLM drivers in a chain. If the primary driver fails, the next driver in the chain is tried automatically.

### Behavior

- On success: returns immediately
- On **rate limit / overload** errors (`429`, `529`): bubbles up for retry logic (does NOT failover, because the primary should be retried after backoff)
- On **all other errors**: logs a warning and tries the next driver in the chain
- If all drivers fail: returns the last error

### Configuration

Fallback chains are configured in your agent manifest or `config.toml`. The `FallbackDriver` is used automatically when an agent is in **Stabilisation mode** (`KernelMode::Stable`) or when multiple providers are configured for reliability.

```toml
# Example: primary Anthropic, fallback to Gemini, then Groq
[[agents]]
name = "production-bot"
model = "claude-sonnet-4-20250514"
fallback_models = ["gemini-2.5-flash", "llama-3.3-70b-versatile"]
```

The fallback driver creates a chain: `AnthropicDriver -> GeminiDriver -> OpenAIDriver(Groq)`.

---

## API Endpoints

### List All Models

```
GET /api/models
```

Returns the complete model catalog with metadata, pricing, and feature flags.

**Response:**
```json
[
  {
    "id": "claude-sonnet-4-20250514",
    "display_name": "Claude Sonnet 4",
    "provider": "anthropic",
    "tier": "Smart",
    "context_window": 200000,
    "max_output_tokens": 64000,
    "input_cost_per_m": 3.0,
    "output_cost_per_m": 15.0,
    "supports_tools": true,
    "supports_vision": true,
    "supports_streaming": true,
    "aliases": ["sonnet", "claude-sonnet"]
  }
]
```

### Get Specific Model

```
GET /api/models/{id}
```

Returns a single model entry. Supports both canonical IDs and aliases.

```
GET /api/models/sonnet
GET /api/models/claude-sonnet-4-20250514
```

### List Aliases

```
GET /api/models/aliases
```

Returns a map of all alias-to-canonical-ID mappings.

**Response:**
```json
{
  "sonnet": "claude-sonnet-4-20250514",
  "haiku": "claude-haiku-4-5-20251001",
  "flash": "gemini-2.5-flash",
  "grok": "grok-2"
}
```

### List Providers

```
GET /api/providers
```

Returns all 20 providers with auth status and model counts.

**Response:**
```json
[
  {
    "id": "anthropic",
    "display_name": "Anthropic",
    "api_key_env": "ANTHROPIC_API_KEY",
    "base_url": "https://api.anthropic.com",
    "key_required": true,
    "auth_status": "Configured",
    "model_count": 3
  },
  {
    "id": "ollama",
    "display_name": "Ollama",
    "api_key_env": "OLLAMA_API_KEY",
    "base_url": "http://localhost:11434/v1",
    "key_required": false,
    "auth_status": "NotRequired",
    "model_count": 5
  }
]
```

Auth status values: `Configured`, `Missing`, `NotRequired`.

### Set Provider API Key

```
POST /api/providers/{name}/key
Content-Type: application/json

{ "api_key": "sk-..." }
```

Configures an API key for a provider at runtime (stored as a `Zeroizing<String>`, wiped from memory on drop).

### Remove Provider API Key

```
DELETE /api/providers/{name}/key
```

Removes the configured API key for a provider.

### Test Provider Connection

```
POST /api/providers/{name}/test
```

Sends a minimal test request to verify the provider is reachable and the API key is valid.

---

## Channel Commands

Two chat commands are available in any channel for inspecting models and providers:

### `/models`

Lists all available models with their tier, provider, and context window. Only shows models from providers that have authentication configured (or do not require it).

```
/models
```

Example output:
```
Available models (12):

Frontier:
  claude-opus-4-20250514 (Anthropic) — 200K ctx
  gemini-2.5-pro (Google Gemini) — 1M ctx

Smart:
  claude-sonnet-4-20250514 (Anthropic) — 200K ctx
  gemini-2.5-flash (Google Gemini) — 1M ctx
  deepseek-chat (DeepSeek) — 64K ctx

Balanced:
  llama-3.3-70b-versatile (Groq) — 128K ctx

Fast:
  claude-haiku-4-5-20251001 (Anthropic) — 200K ctx
  gemini-2.0-flash (Google Gemini) — 1M ctx

Local:
  llama3.2 (Ollama) — 128K ctx
```

### `/providers`

Lists all 20 providers with their authentication status.

```
/providers
```

Example output:
```
LLM Providers (20):

  Anthropic          ANTHROPIC_API_KEY       Configured    3 models
  OpenAI             OPENAI_API_KEY          Missing       6 models
  Google Gemini      GEMINI_API_KEY          Configured    3 models
  DeepSeek           DEEPSEEK_API_KEY        Missing       2 models
  Groq               GROQ_API_KEY            Configured    4 models
  Ollama             (no key needed)         Ready         3 models
  vLLM               (no key needed)         Ready         1 model
  LM Studio          (no key needed)         Ready         1 model
  ...
```

---

## Environment Variables Summary

Quick reference for all provider environment variables:

| Provider | Env Var | Required |
|----------|---------|----------|
| Anthropic | `ANTHROPIC_API_KEY` | Yes |
| OpenAI | `OPENAI_API_KEY` | Yes |
| Google Gemini | `GEMINI_API_KEY` or `GOOGLE_API_KEY` | Yes |
| DeepSeek | `DEEPSEEK_API_KEY` | Yes |
| Groq | `GROQ_API_KEY` | Yes |
| OpenRouter | `OPENROUTER_API_KEY` | Yes |
| Mistral AI | `MISTRAL_API_KEY` | Yes |
| Together AI | `TOGETHER_API_KEY` | Yes |
| Fireworks AI | `FIREWORKS_API_KEY` | Yes |
| Ollama | `OLLAMA_API_KEY` | No |
| vLLM | `VLLM_API_KEY` | No |
| LM Studio | `LMSTUDIO_API_KEY` | No |
| Perplexity AI | `PERPLEXITY_API_KEY` | Yes |
| Cohere | `COHERE_API_KEY` | Yes |
| AI21 Labs | `AI21_API_KEY` | Yes |
| Cerebras | `CEREBRAS_API_KEY` | Yes |
| SambaNova | `SAMBANOVA_API_KEY` | Yes |
| Hugging Face | `HF_API_KEY` | Yes |
| xAI | `XAI_API_KEY` | Yes |
| Replicate | `REPLICATE_API_TOKEN` | Yes |

---

## Security Notes

- All API keys are stored as `Zeroizing<String>` -- the key material is automatically overwritten with zeros when the value is dropped from memory.
- Auth detection (`detect_auth()`) only checks `std::env::var()` for presence -- it never reads or logs the actual secret value.
- Provider API keys set via the REST API (`POST /api/providers/{name}/key`) follow the same zeroization policy.
- The health endpoint (`/api/health`) never exposes provider auth status or API keys. Detailed info is behind `/api/health/detail` which requires authentication.
- All `DriverConfig` and `KernelConfig` structs implement `Debug` with secret redaction -- API keys are printed as `"***"` in logs.
