# Providers And Models

This page is the top-level routing guide for choosing an LLM provider strategy before you dive into the full [Providers](providers.md) reference.

---

## Start Here

- Read [Providers](providers.md) for the full provider catalog, model list, routing, and cost details.
- Read [API Surfaces](api-surfaces.md) if you are choosing providers because of a specific API contract.
- Read [Integrations](integrations.md) for the higher-level integration-path decision.

## Fastest Provider Paths

### Fastest Local Validation

Start here when you want the least setup friction for local development.

- Groq: fast hosted inference with a usable free tier
- Gemini: generous free tier and strong general-purpose quality

Best fit:

- local bring-up
- quick demos
- docs and example validation

### Premium Hosted Quality

Start here when you care more about model quality and ecosystem maturity than lowest-friction bring-up.

- Anthropic
- OpenAI
- Gemini Pro

Best fit:

- production assistants
- premium reasoning quality
- mature hosted provider tooling

### Broadest Model Choice

Start here when you want one gateway to many upstream models.

- OpenRouter

Best fit:

- teams still evaluating model mix
- fast experimentation across providers
- one API key with multiple model options

### Local Or Private Deployment

Start here when data locality or offline operation matters.

- Ollama
- vLLM
- LM Studio

Best fit:

- local prototyping
- private infrastructure
- self-hosted model access

## Choose By Job

- Choose Groq or Gemini for low-friction setup.
- Choose Anthropic or OpenAI for premium hosted quality.
- Choose OpenRouter for broad routing flexibility.
- Choose Ollama, vLLM, or LM Studio for local or private deployments.
- Choose multiple providers when fallback behavior matters more than a single default model.

## What The Detailed Guide Covers

The full [Providers](providers.md) guide includes:

- provider-by-provider setup
- model catalog and aliases
- per-agent overrides
- model routing heuristics
- cost tracking and quotas
- fallback provider behavior
- provider and model API endpoints

## Related API Surfaces

The main provider-facing and model-facing routes are:

- `GET /api/models`
- `GET /api/models/{id}`
- `GET /api/models/aliases`
- `GET /api/providers`
- `POST /api/providers/{name}/key`
- `DELETE /api/providers/{name}/key`
- `POST /api/providers/{name}/test`
- `GET /v1/models`

For route selection, read [API Surfaces](api-surfaces.md).

## Next Step

After choosing the provider strategy, use [API Surfaces](api-surfaces.md) to pick the right protocol or endpoint family and then use [Integrations](integrations.md) to wire the surrounding application boundary.
