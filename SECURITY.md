# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.6.x   | :white_check_mark: |
| 0.3.x   | :white_check_mark: |

## Hardening primitives (v0.6.1)

OpenFang's v0.6.1 hardening pass introduced several primitives that callers (kernel, API layer) build on. Operators should be aware of the contracts even if their concrete wire-up is still in flight per `docs/hardening-status.md`.

### Untrusted-content channel

Every external input — web fetch, MCP tool result, Obsidian read, channel-inbound message, file read — must flow through `openfang_runtime::untrusted::wrap(source, body)` before it reaches an LLM.

- **SHA256-delimited boundaries.** Each wrapped block opens with `<<<EXTCONTENT_<12 hex chars>>>` and closes with `<<</EXTCONTENT_<same hex>>>`. The hex prefix is `SHA256(source)[..6]`, so two different sources never collide; models can rely on the boundary to scope trust.
- **Jailbreak-marker stripping.** 16 known role/template delimiters are neutralised before wrapping: `<|im_start|>`, `<|im_end|>`, `<|endoftext|>`, `<|system|>`, `<|assistant|>`, `<|user|>`, `</s>`, the Phase-3 persona delimiters `<persona>` / `</persona>`, the tool-call delimiters consumed by `recover_text_tool_calls` (`<tool_use>`, `<tool_call>`, `<function_call>` and their closing tags), and Anthropic-style `<thinking>` tags. Each is rewritten to a `[bracketed]` literal so the chat-template parser on the provider side can't misinterpret the boundary.
- **Quarantine first.** `untrusted::quarantine_write(base, agent_id, source, body)` writes raw bytes to `<base>/<agent_id>/<sha-prefix>/{body.bin, source.txt}` BEFORE any other processing. Default base is `$XDG_DATA_HOME/openfang/quarantine`, falling back to `~/.openfang/quarantine`. Agent-id is constrained to `[a-zA-Z0-9_-]{1,80}` and the resolved path is canonicalised + verified to live under the base — refused otherwise.

### Triage pipeline

Quarantined content runs through:

1. `triage::heuristic::HeuristicScanner` — 12 regex rules across jailbreak preludes, credential exfil (AWS, Google, private-key headers, Bearer tokens), SSRF / cloud-metadata service abuse (AWS / GCP / Azure IMDS endpoints), and obfuscation (base64-pipe-shell, `eval(atob(...))`, `String.fromCharCode` cascades).
2. `triage::moonlock::MoonlockDeepscanner` — shells out to the operator's Moonlock CLI (discovered via `OPENFANG_MOONLOCK_PATH` or `which moonlock`) with a 30-second timeout. Permissive verdict-alias parser; every failure path (binary missing, spawn failed, timeout, non-zero exit, empty stdout, parse error, missing verdict, unknown verdict) is categorised in `findings`.
3. `triage::classifier::run_classifier(...)` — the bundled cyber-agent (`agents/cyber/`) is a frontier-model-only LLM (claude-opus-4-7, temperature 0.0) that ingests scanner outcomes + content summary + cyber-intel excerpts and returns `{verdict, rationale, recommended_action, confidence}`. Strict JSON output with `deny_unknown_fields` and range-checked confidence.
4. `triage::pinboard::PinboardStore` holds Suspicious / ScanFailed / Questionable items between classifier verdict and operator decision. State machine: `Pending → Allowed via Allow`, `Pending → Quarantined via Quarantine`, `Comment` is audit-only. Reverse transitions (`Allowed → Quarantined`) are explicitly refused — a release cannot be silently undone.

The pipeline is **fail-closed end to end**: any scanner error, any classifier LLM error, any parse failure routes the content to the pinboard rather than memory. `Verdict::worst_of` enforces `Malicious > ScanFailed > Suspicious > Safe` precedence so a scan failure never quietly degrades to Safe.

### Local LLM endpoint policy

Local LLM providers (Ollama, vLLM, LM Studio, Lemonade) typically run with no auth, so a non-loopback `base_url` exposes the agent's tool-calling surface to whatever else can reach the configured host.

- **Default-deny non-loopback.** `provider = "ollama"` with `base_url = "http://192.168.1.5:11434"` is refused at driver construction. Loopback is detected via `url::Host` enum: IPv4 `is_loopback()` / `is_unspecified()`, IPv6 `is_loopback()` / `is_unspecified()`, or literal domain `localhost`.
- **Opt-in override.** Set `OPENFANG_OLLAMA_ALLOW_NON_LOOPBACK=1` for trusted-LAN deployments (e.g. a private jump-box that's known not to expose Ollama publicly).
- **Model-discovery enrichment.** Opaque `model not found` errors from Ollama now resolve through `/api/tags` and surface as a typed `ModelNotFound` listing the actually-pulled models.

### Persona injection defence

The system prompt assembled by `prompt_builder.rs::build_persona_section` now wraps SOUL.md content in explicit `<persona>…</persona>` tags. Body text from SOUL.md is sanitised by `soul::format_persona_block` so a hostile edit cannot emit a literal `</persona>` to close the outer tag early and inject "outside" the persona block. Tool output that arrives later in the conversation (which has been wrapped by `untrusted::wrap`) cannot impersonate persona content because it sits outside the tags.

### Soul reflection guards

The `reflection.rs` self-update pipeline that mutates `SOUL.md` enforces:

- **Cadence:** at most 4 reflections per rolling 24h, minimum 4h between reflections (`MAX_REFLECTIONS_PER_WINDOW`, `MIN_GAP_SECONDS`).
- **Two-phase commit:** reflection patches land in `soul_patch_proposal.md` first; the live SOUL.md is only mutated on the next agent boot, after the proposal round-trips cleanly through the parser.
- **Immutable fields:** any patch that would mutate `name`, `archetype`, `values`, or `non_negotiables` is rejected by `check_immutable_fields` regardless of how it sneaks through the parser. Memory-focus and last-reflection-at are the only mutable fields.
- **Strict JSON parser:** `deny_unknown_fields` blocks any field the schema doesn't whitelist, and item-count + char-length caps stop runaway output.

### Mempalace required

The `MempalaceBackend` ships with `Criticality::Critical` by default. The kernel's boot-warm path calls `verify_boot()` and refuses to finish boot if the mempalace MCP is unreachable, surfacing the verbatim remediation pointing at `~/Library/Mobile Documents/com~apple~CloudDocs/mempalace/INTEGRATION_PLAN.md`.



## Reporting a Vulnerability

If you discover a security vulnerability in OpenFang, please report it responsibly.

**Do NOT open a public GitHub issue for security vulnerabilities.**

### How to Report

1. Email: **jaber@rightnowai.co**
2. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Affected versions
   - Potential impact assessment
   - Suggested fix (if any)

### What to Expect

- **Acknowledgment** within 48 hours
- **Initial assessment** within 7 days
- **Fix timeline** communicated within 14 days
- **Credit** given in the advisory (unless you prefer anonymity)

### Scope

The following are in scope for security reports:

- Authentication/authorization bypass
- Remote code execution
- Path traversal / directory traversal
- Server-Side Request Forgery (SSRF)
- Privilege escalation between agents or users
- Information disclosure (API keys, secrets, internal state)
- Denial of service via resource exhaustion
- Supply chain attacks via skill ecosystem
- WASM sandbox escapes

## Security Architecture

OpenFang implements defense-in-depth with the following security controls:

### Access Control
- **Capability-based permissions**: Agents only access resources explicitly granted
- **RBAC multi-user**: Owner/Admin/User/Viewer role hierarchy
- **Privilege escalation prevention**: Child agents cannot exceed parent capabilities
- **API authentication**: Bearer token with loopback bypass for local CLI

### Input Validation
- **Path traversal protection**: `safe_resolve_path()` / `safe_resolve_parent()` on all file operations
- **SSRF protection**: Private IP blocking, DNS resolution checks, cloud metadata endpoint filtering
- **Image validation**: Media type whitelist (png/jpeg/gif/webp), 5MB size limit
- **Prompt injection scanning**: Skill content scanned for override attempts and data exfiltration

### Cryptographic Security
- **Ed25519 signed manifests**: Agent identity verification
- **HMAC-SHA256 wire protocol**: Mutual authentication with nonce-based replay protection
- **Secret zeroization**: `Zeroizing<String>` on all API key fields, wiped on drop

### Runtime Isolation
- **WASM dual metering**: Fuel limits + epoch interruption with watchdog thread
- **Subprocess sandbox**: Environment isolation (`env_clear()`), restricted PATH
- **Taint tracking**: Information flow labels prevent untrusted data in privileged operations

### Network Security
- **GCRA rate limiter**: Cost-aware token buckets per IP
- **Security headers**: CSP, X-Frame-Options, X-Content-Type-Options, HSTS
- **Health redaction**: Public endpoint returns minimal info; full diagnostics require auth
- **CORS policy**: Restricted to localhost when no API key configured

### Audit
- **Merkle hash chain**: Tamper-evident audit trail for all agent actions
- **Tamper detection**: Chain integrity verification via `/api/audit/verify`

## Dependencies

Security-critical dependencies are pinned and audited:

| Dependency | Purpose |
|------------|---------|
| `ed25519-dalek` | Manifest signing |
| `sha2` | Hash chain, checksums |
| `hmac` | Wire protocol authentication |
| `subtle` | Constant-time comparison |
| `zeroize` | Secret memory wiping |
| `rand` | Cryptographic randomness |
| `governor` | Rate limiting |
