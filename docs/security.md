# OpenFang Security Architecture

> **Security Contact:** jaber@rightnowai.co — Report vulnerabilities via email. We respond within 48 hours.

This document provides a comprehensive technical reference for every security
system in the OpenFang Agent Operating System.  All struct names, function
signatures, constant values, and algorithm descriptions are drawn directly from
the source code.

---

## Table of Contents

1.  [Security Overview](#1-security-overview)
2.  [Capability-Based Security](#2-capability-based-security)
3.  [WASM Dual Metering](#3-wasm-dual-metering)
4.  [Merkle Hash Chain Audit Trail](#4-merkle-hash-chain-audit-trail)
5.  [Information Flow Taint Tracking](#5-information-flow-taint-tracking)
6.  [Ed25519 Manifest Signing](#6-ed25519-manifest-signing)
7.  [SSRF Protection](#7-ssrf-protection)
8.  [Secret Zeroization](#8-secret-zeroization)
9.  [OFP Mutual Authentication](#9-ofp-mutual-authentication)
10. [Security Headers](#10-security-headers)
11. [GCRA Rate Limiter](#11-gcra-rate-limiter)
12. [Path Traversal Prevention](#12-path-traversal-prevention)
13. [Subprocess Sandbox](#13-subprocess-sandbox)
14. [Prompt Injection Scanner](#14-prompt-injection-scanner)
15. [Loop Guard](#15-loop-guard)
16. [Session Repair](#16-session-repair)
17. [Health Endpoint Redaction](#17-health-endpoint-redaction)
18. [Security Configuration](#18-security-configuration)
19. [Security Dependencies](#19-security-dependencies)

---

## 1. Security Overview

OpenFang implements **defense-in-depth** security.  No single mechanism is
trusted to be the sole protector; instead, 16 independent systems form
overlapping layers so that a failure in any one layer is caught by others.

| # | System | Crate | Protects Against |
|---|--------|-------|------------------|
| 1 | Capability-Based Security | `openfang-types` | Unauthorized actions by agents |
| 2 | WASM Dual Metering | `openfang-runtime` | Infinite loops, CPU DoS |
| 3 | Merkle Audit Trail | `openfang-runtime` | Tampered audit logs |
| 4 | Taint Tracking | `openfang-types` | Prompt injection, data exfiltration |
| 5 | Ed25519 Manifest Signing | `openfang-types` | Supply chain attacks |
| 6 | SSRF Protection | `openfang-runtime` | Server-Side Request Forgery |
| 7 | Secret Zeroization | `openfang-runtime`, `openfang-channels` | Memory forensics, key leakage |
| 8 | OFP Mutual Auth | `openfang-wire` | Unauthorized peer connections |
| 9 | Security Headers | `openfang-api` | XSS, clickjacking, MIME sniffing |
| 10 | GCRA Rate Limiter | `openfang-api` | API abuse, denial of service |
| 11 | Path Traversal Prevention | `openfang-runtime` | Directory traversal attacks |
| 12 | Subprocess Sandbox | `openfang-runtime` | Secret leakage via child processes |
| 13 | Prompt Injection Scanner | `openfang-skills` | Malicious skill prompts |
| 14 | Loop Guard | `openfang-runtime` | Stuck agent tool loops |
| 15 | Session Repair | `openfang-runtime` | Corrupted LLM conversation history |
| 16 | Health Endpoint Redaction | `openfang-api` | Information leakage |

---

## 2. Capability-Based Security

**Source:** `openfang-types/src/capability.rs`

OpenFang uses capability-based security.  An agent can only perform actions
it has been explicitly granted permission to do.  Capabilities are immutable
after agent creation and are enforced at the kernel level.

### 2.1 Capability Variants

The `Capability` enum defines every permission type:

```rust
pub enum Capability {
    // Filesystem
    FileRead(String),       // Glob pattern, e.g. "/data/*"
    FileWrite(String),

    // Network
    NetConnect(String),     // Host:port pattern, e.g. "*.openai.com:443"
    NetListen(u16),

    // Tools
    ToolInvoke(String),     // Specific tool ID
    ToolAll,                // All tools (dangerous)

    // LLM
    LlmQuery(String),
    LlmMaxTokens(u64),

    // Agent interaction
    AgentSpawn,
    AgentMessage(String),
    AgentKill(String),

    // Memory
    MemoryRead(String),
    MemoryWrite(String),

    // Shell
    ShellExec(String),
    EnvRead(String),

    // OFP Wire Protocol
    OfpDiscover,
    OfpConnect(String),
    OfpAdvertise,

    // Economic
    EconSpend(f64),
    EconEarn,
    EconTransfer(String),
}
```

### 2.2 Pattern Matching

The `capability_matches(granted, required)` function implements glob-style
matching:

- **Exact match:** `"api.openai.com:443"` matches `"api.openai.com:443"`
- **Full wildcard:** `"*"` matches anything
- **Prefix wildcard:** `"*.openai.com:443"` matches `"api.openai.com:443"`
- **Suffix wildcard:** `"api.*"` matches `"api.openai.com"`
- **Middle wildcard:** `"api.*.com"` matches `"api.openai.com"`
- **ToolAll special case:** `ToolAll` grants any `ToolInvoke(_)`
- **Numeric bounds:** `LlmMaxTokens(10000)` grants `LlmMaxTokens(5000)` (granted >= required)

### 2.3 Enforcement Point

In the WASM sandbox, every host call is checked **before** execution by
`check_capability()` in `host_functions.rs`:

```rust
fn check_capability(
    capabilities: &[Capability],
    required: &Capability,
) -> Result<(), serde_json::Value> {
    for granted in capabilities {
        if capability_matches(granted, required) {
            return Ok(());
        }
    }
    Err(json!({"error": format!("Capability denied: {required:?}")}))
}
```

If no granted capability matches the required one, the operation returns a
JSON error immediately -- the tool is never invoked.

### 2.4 Capability Inheritance

When an agent spawns a child agent, `validate_capability_inheritance()` ensures
the child's capabilities are a **subset** of the parent's.  This prevents
privilege escalation:

```rust
pub fn validate_capability_inheritance(
    parent_caps: &[Capability],
    child_caps: &[Capability],
) -> Result<(), String> {
    for child_cap in child_caps {
        let is_covered = parent_caps
            .iter()
            .any(|parent_cap| capability_matches(parent_cap, child_cap));
        if !is_covered {
            return Err(format!(
                "Privilege escalation denied: child requests {:?} \
                 but parent does not have a matching grant",
                child_cap
            ));
        }
    }
    Ok(())
}
```

The `host_agent_spawn()` function in `host_functions.rs` calls
`kernel.spawn_agent_checked(manifest_toml, Some(&state.agent_id), &state.capabilities)`
which invokes this validation before the child is created.

---

## 3. WASM Dual Metering

**Source:** `openfang-runtime/src/sandbox.rs`

Untrusted WASM modules run inside a Wasmtime sandbox with **two
independent** metering mechanisms running simultaneously.

### 3.1 Fuel Metering (Deterministic)

Fuel metering counts WASM instructions.  The engine deducts fuel for every
instruction executed.  When the budget is exhausted, execution traps with
`Trap::OutOfFuel`.

```rust
// SandboxConfig defaults
pub fuel_limit: u64,  // Default: 1_000_000

// Applied at execution time
if config.fuel_limit > 0 {
    store.set_fuel(config.fuel_limit)?;
}
```

After execution, fuel consumed is reported:

```rust
let fuel_remaining = store.get_fuel().unwrap_or(0);
let fuel_consumed = config.fuel_limit.saturating_sub(fuel_remaining);
```

### 3.2 Epoch Interruption (Wall-Clock)

A watchdog thread sleeps for the configured timeout, then increments the
engine epoch.  When the epoch advances past the store's deadline, execution
traps with `Trap::Interrupt`.

```rust
store.set_epoch_deadline(1);
let engine_clone = engine.clone();
let timeout = config.timeout_secs.unwrap_or(30);
let _watchdog = std::thread::spawn(move || {
    std::thread::sleep(std::time::Duration::from_secs(timeout));
    engine_clone.increment_epoch();
});
```

### 3.3 Why Both?

| Property | Fuel | Epoch |
|----------|------|-------|
| **Metric** | Instruction count | Wall-clock time |
| **Precision** | Deterministic, reproducible | Non-deterministic |
| **Catches** | CPU-intensive loops | Host call blocking, I/O waits |
| **Evasion** | Can waste time in host calls | Can busy-loop cheaply |

Together they form a complete defense: fuel catches compute-intensive loops,
while epochs catch host-call abuse or environmental slowdowns.

### 3.4 SandboxConfig

```rust
pub struct SandboxConfig {
    pub fuel_limit: u64,           // Default: 1_000_000
    pub max_memory_bytes: usize,   // Default: 16 MB
    pub capabilities: Vec<Capability>,
    pub timeout_secs: Option<u64>, // Default: 30 seconds
}
```

### 3.5 Error Types

```rust
pub enum SandboxError {
    Compilation(String),
    Instantiation(String),
    Execution(String),
    FuelExhausted,         // Trap::OutOfFuel
    AbiError(String),
}
```

---

## 4. Merkle Hash Chain Audit Trail

**Source:** `openfang-runtime/src/audit.rs`

Every security-critical action is appended to a tamper-evident Merkle hash
chain, similar to a blockchain.  Each entry contains the SHA-256 hash of its
own contents concatenated with the hash of the previous entry.

### 4.1 Auditable Actions

```rust
pub enum AuditAction {
    ToolInvoke,
    CapabilityCheck,
    AgentSpawn,
    AgentKill,
    AgentMessage,
    MemoryAccess,
    FileAccess,
    NetworkAccess,
    ShellExec,
    AuthAttempt,
    WireConnect,
    ConfigChange,
}
```

### 4.2 Entry Structure

```rust
pub struct AuditEntry {
    pub seq: u64,          // Monotonically increasing sequence number
    pub timestamp: String, // ISO-8601
    pub agent_id: String,
    pub action: AuditAction,
    pub detail: String,    // e.g. tool name, file path
    pub outcome: String,   // "ok", "denied", error message
    pub prev_hash: String, // SHA-256 of previous entry (or 64 zeros)
    pub hash: String,      // SHA-256 of this entry + prev_hash
}
```

### 4.3 Hash Computation

Each entry's hash is computed from all of its fields concatenated with the
previous entry's hash:

```rust
fn compute_entry_hash(
    seq: u64, timestamp: &str, agent_id: &str,
    action: &AuditAction, detail: &str,
    outcome: &str, prev_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seq.to_string().as_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.update(agent_id.as_bytes());
    hasher.update(action.to_string().as_bytes());
    hasher.update(detail.as_bytes());
    hasher.update(outcome.as_bytes());
    hasher.update(prev_hash.as_bytes());
    hex::encode(hasher.finalize())
}
```

### 4.4 Chain Integrity Verification

`AuditLog::verify_integrity()` walks the entire chain and recomputes every
hash.  If any entry has been tampered with, the recomputed hash will not match
the stored hash, or the `prev_hash` linkage will be broken:

```rust
pub fn verify_integrity(&self) -> Result<(), String> {
    let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
    let mut expected_prev = "0".repeat(64);  // Genesis sentinel

    for entry in entries.iter() {
        if entry.prev_hash != expected_prev {
            return Err(format!(
                "chain break at seq {}: expected prev_hash {} but found {}",
                entry.seq, expected_prev, entry.prev_hash
            ));
        }
        let recomputed = compute_entry_hash(/* ... */);
        if recomputed != entry.hash {
            return Err(format!(
                "hash mismatch at seq {}: expected {} but found {}",
                entry.seq, recomputed, entry.hash
            ));
        }
        expected_prev = entry.hash.clone();
    }
    Ok(())
}
```

### 4.5 Thread Safety

`AuditLog` uses `Mutex<Vec<AuditEntry>>` and `Mutex<String>` for the tip hash.
Both locks use `unwrap_or_else(|e| e.into_inner())` to recover from poisoned
mutexes, ensuring the audit log remains available even after a panic.

### 4.6 API

| Method | Description |
|--------|-------------|
| `AuditLog::new()` | Creates an empty log with genesis sentinel (`"0" * 64`) |
| `record(agent_id, action, detail, outcome)` | Appends an entry, returns its hash |
| `verify_integrity()` | Validates the entire chain |
| `tip_hash()` | Returns the hash of the most recent entry |
| `len()` / `is_empty()` | Entry count |
| `recent(n)` | Returns the most recent `n` entries (cloned) |

---

## 5. Information Flow Taint Tracking

**Source:** `openfang-types/src/taint.rs`

OpenFang implements a lattice-based taint propagation model that prevents
tainted values from flowing into sensitive sinks without explicit
declassification.  This guards against prompt injection, data exfiltration,
and confused-deputy attacks.

### 5.1 Taint Labels

```rust
pub enum TaintLabel {
    ExternalNetwork,  // Data from external network requests
    UserInput,        // Direct user input
    Pii,              // Personally identifiable information
    Secret,           // API keys, tokens, passwords
    UntrustedAgent,   // Data from sandboxed/untrusted agents
}
```

### 5.2 Tainted Values

```rust
pub struct TaintedValue {
    pub value: String,              // The payload
    pub labels: HashSet<TaintLabel>, // Attached taint labels
    pub source: String,             // Human-readable origin
}
```

Key methods:

| Method | Description |
|--------|-------------|
| `TaintedValue::new(value, labels, source)` | Create with labels |
| `TaintedValue::clean(value, source)` | Create with no labels (untainted) |
| `merge_taint(&mut self, other)` | Union of labels (for concatenation) |
| `check_sink(&self, sink)` | Check if value can flow to sink |
| `declassify(&mut self, label)` | Remove a specific label (explicit security decision) |
| `is_tainted(&self) -> bool` | True if any labels present |

### 5.3 Taint Sinks

A `TaintSink` defines which labels are **blocked** from reaching it:

| Sink | Blocked Labels | Rationale |
|------|---------------|-----------|
| `TaintSink::shell_exec()` | `ExternalNetwork`, `UntrustedAgent`, `UserInput` | Prevents command injection |
| `TaintSink::net_fetch()` | `Secret`, `Pii` | Prevents data exfiltration |
| `TaintSink::agent_message()` | `Secret` | Prevents secret leakage to other agents |

### 5.4 Violation Handling

When `check_sink()` finds a blocked label, it returns a `TaintViolation`:

```rust
pub struct TaintViolation {
    pub label: TaintLabel,    // The offending label
    pub sink_name: String,    // "shell_exec", "net_fetch", etc.
    pub source: String,       // Where the tainted value came from
}
```

Display: `taint violation: label 'Secret' from source 'env_var' is not allowed to reach sink 'net_fetch'`

### 5.5 Declassification

Declassification is an **explicit security decision**.  The caller asserts
that the value has been sanitized:

```rust
tainted.declassify(&TaintLabel::ExternalNetwork);
tainted.declassify(&TaintLabel::UserInput);
// After declassification, value can flow to shell_exec
assert!(tainted.check_sink(&TaintSink::shell_exec()).is_ok());
```

### 5.6 Taint Propagation

When two values are combined (concatenation, interpolation), the result must
carry the union of both label sets:

```rust
let mut combined = TaintedValue::new(/* ... */);
combined.merge_taint(&other_value);
// combined.labels is now the union of both
```

---

## 6. Ed25519 Manifest Signing

**Source:** `openfang-types/src/manifest_signing.rs`

Agent manifests define an agent's capabilities, tools, and configuration.
A compromised manifest can grant elevated privileges.  This module provides
Ed25519-based cryptographic signing.

### 6.1 Signing Scheme

1. Compute SHA-256 of the manifest content (raw TOML text).
2. Sign the hash with Ed25519 (via `ed25519-dalek`).
3. Bundle the signature, public key, and content hash into a `SignedManifest` envelope.

### 6.2 SignedManifest Structure

```rust
pub struct SignedManifest {
    pub manifest: String,           // Raw TOML content
    pub content_hash: String,       // Hex SHA-256 of manifest
    pub signature: Vec<u8>,         // Ed25519 signature (64 bytes)
    pub signer_public_key: Vec<u8>, // Ed25519 public key (32 bytes)
    pub signer_id: String,          // Human-readable signer ID
}
```

### 6.3 Signing

```rust
let signing_key = SigningKey::generate(&mut OsRng);
let signed = SignedManifest::sign(manifest_toml, &signing_key, "admin@org.com");
```

Internally:

```rust
pub fn sign(manifest: impl Into<String>, signing_key: &SigningKey, signer_id: impl Into<String>) -> Self {
    let manifest = manifest.into();
    let content_hash = hash_manifest(&manifest);  // SHA-256
    let signature = signing_key.sign(content_hash.as_bytes());
    let verifying_key = signing_key.verifying_key();
    Self {
        manifest,
        content_hash,
        signature: signature.to_bytes().to_vec(),
        signer_public_key: verifying_key.to_bytes().to_vec(),
        signer_id: signer_id.into(),
    }
}
```

### 6.4 Verification

Two-phase verification:

1. **Hash check:** Recompute SHA-256 of `manifest` and compare to `content_hash`.
2. **Signature check:** Verify the Ed25519 signature over `content_hash` using `signer_public_key`.

```rust
pub fn verify(&self) -> Result<(), String> {
    let recomputed = hash_manifest(&self.manifest);
    if recomputed != self.content_hash {
        return Err("content hash mismatch: ...");
    }
    let verifying_key = VerifyingKey::from_bytes(&pk_bytes)?;
    let signature = Signature::from_bytes(&sig_bytes);
    verifying_key.verify(self.content_hash.as_bytes(), &signature)
        .map_err(|e| format!("signature verification failed: {}", e))
}
```

### 6.5 Tamper Detection

- Modifying the manifest content after signing causes a **content hash mismatch**.
- Replacing the public key with a different key causes a **signature verification failure**.
- Both attacks are caught by `verify()`.

---

## 7. SSRF Protection

**Source:** `openfang-runtime/src/host_functions.rs`

The `host_net_fetch` function (WASM host call for network requests) includes
comprehensive Server-Side Request Forgery protection.

### 7.1 Scheme Validation

Only `http://` and `https://` schemes are allowed.  All others (`file://`,
`gopher://`, `ftp://`) are blocked immediately:

```rust
if !url.starts_with("http://") && !url.starts_with("https://") {
    return Err(json!({"error": "Only http:// and https:// URLs are allowed"}));
}
```

### 7.2 Hostname Blocklist

Before DNS resolution, these hostnames are blocked:

- `localhost`
- `metadata.google.internal`
- `metadata.aws.internal`
- `instance-data`
- `169.254.169.254` (AWS/GCP metadata endpoint)

### 7.3 DNS Resolution Check

After the hostname blocklist, the function resolves the hostname to IP
addresses and checks **every resolved IP** against private ranges.  This
defeats DNS rebinding attacks:

```rust
let socket_addr = format!("{hostname}:{port}");
if let Ok(addrs) = socket_addr.to_socket_addrs() {
    for addr in addrs {
        let ip = addr.ip();
        if ip.is_loopback() || ip.is_unspecified() || is_private_ip(&ip) {
            return Err(json!({"error": format!(
                "SSRF blocked: {hostname} resolves to private IP {ip}"
            )}));
        }
    }
}
```

### 7.4 Private IP Detection

The `is_private_ip()` function covers:

**IPv4:**
- `10.0.0.0/8` -- RFC 1918
- `172.16.0.0/12` -- RFC 1918
- `192.168.0.0/16` -- RFC 1918
- `169.254.0.0/16` -- Link-local (AWS metadata)

**IPv6:**
- `fc00::/7` -- Unique Local Address
- `fe80::/10` -- Link-local

```rust
fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            matches!(
                octets,
                [10, ..] | [172, 16..=31, ..] | [192, 168, ..] | [169, 254, ..]
            )
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
        }
    }
}
```

### 7.5 Host Extraction

`extract_host_from_url()` parses the URL to extract `host:port` for both
SSRF checking and capability matching:

```
https://api.openai.com/v1/chat  ->  api.openai.com:443
http://localhost:8080/api       ->  localhost:8080
http://example.com              ->  example.com:80
```

---

## 8. Secret Zeroization

**Source:** All LLM driver modules, channel adapters, and web search modules.

OpenFang uses `Zeroizing<String>` from the `zeroize` crate on every field
that holds secret material.  When the value is dropped, its memory is
overwritten with zeros, preventing secrets from lingering in memory.

### 8.1 How It Works

`Zeroizing<T>` is a smart-pointer wrapper from the `zeroize` crate.  It
implements `Deref<Target=T>` for transparent usage and `Drop` for automatic
zeroization:

```rust
// On Drop, the inner String's buffer is overwritten with zeros
let key = Zeroizing::new("sk-secret-key".to_string());
// Use key transparently via Deref
client.post(url).header("authorization", format!("Bearer {}", &*key));
// When key goes out of scope, memory is zeroed
```

### 8.2 Fields Using Zeroization

**LLM Drivers** (`openfang-runtime/src/drivers/`):

| Driver | Field |
|--------|-------|
| `AnthropicDriver` | `api_key: Zeroizing<String>` |
| `GeminiDriver` | `api_key: Zeroizing<String>` |
| `OpenAiCompatDriver` | `api_key: Zeroizing<String>` |

**Channel Adapters** (`openfang-channels/src/`):

| Adapter | Field(s) |
|---------|----------|
| `DiscordAdapter` | `token: Zeroizing<String>` |
| `EmailAdapter` | `password: Zeroizing<String>` |
| `BlueskyAdapter` | `app_password: Zeroizing<String>` |
| `DingTalkAdapter` | `access_token: Zeroizing<String>`, `secret: Zeroizing<String>` |
| `FeishuAdapter` | `app_secret: Zeroizing<String>` |
| `FlockAdapter` | `bot_token: Zeroizing<String>` |
| `GitterAdapter` | `token: Zeroizing<String>` |
| `GotifyAdapter` | `app_token: Zeroizing<String>`, `client_token: Zeroizing<String>` |

**Web Search** (`openfang-runtime/src/web_search.rs`):

```rust
fn resolve_api_key(env_var: &str) -> Option<Zeroizing<String>> {
    std::env::var(env_var).ok().filter(|k| !k.is_empty()).map(Zeroizing::new)
}
```

**Embedding** (`openfang-runtime/src/embedding.rs`):

| Struct | Field |
|--------|-------|
| `EmbeddingClient` | `api_key: Zeroizing<String>` |

### 8.3 Why It Matters

Without zeroization, secrets remain in memory after use until the OS
reclaims the page.  An attacker with access to a core dump, swap file, or
memory forensics tool can recover API keys.  `Zeroizing<String>` ensures
the secret is overwritten as soon as it is no longer needed.

---

## 9. OFP Mutual Authentication

**Source:** `openfang-wire/src/peer.rs`

The OpenFang Wire Protocol (OFP) uses HMAC-SHA256 with nonce-based mutual
authentication over TCP connections.

### 9.1 Pre-Shared Key Requirement

OFP refuses to start without a `shared_secret`:

```rust
if config.shared_secret.is_empty() {
    return Err(WireError::HandshakeFailed(
        "OFP requires shared_secret. Set [network] shared_secret in config.toml".into(),
    ));
}
```

### 9.2 HMAC Functions

```rust
type HmacSha256 = Hmac<Sha256>;

fn hmac_sign(secret: &str, data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key size");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

fn hmac_verify(secret: &str, data: &[u8], signature: &str) -> bool {
    let expected = hmac_sign(secret, data);
    subtle::ConstantTimeEq::ct_eq(expected.as_bytes(), signature.as_bytes()).into()
}
```

**Constant-time comparison** (`subtle::ConstantTimeEq`) prevents
timing side-channel attacks.

### 9.3 Handshake Protocol

**Initiator (client):**

1. Generate a random UUID nonce.
2. Compute `auth_data = nonce + node_id`.
3. Compute `auth_hmac = hmac_sign(shared_secret, auth_data)`.
4. Send `Handshake { node_id, node_name, protocol_version, agents, nonce, auth_hmac }`.

**Responder (server):**

1. Receive the `Handshake` message.
2. Verify the incoming HMAC: `hmac_verify(shared_secret, nonce + node_id, auth_hmac)`.
3. If verification fails, return error code 403.
4. Generate a new UUID nonce for the ack.
5. Compute `ack_auth_data = ack_nonce + self.node_id`.
6. Compute `ack_hmac = hmac_sign(shared_secret, ack_auth_data)`.
7. Send `HandshakeAck { node_id, node_name, protocol_version, agents, nonce: ack_nonce, auth_hmac: ack_hmac }`.

**Initiator (verification):**

1. Receive `HandshakeAck`.
2. Verify: `hmac_verify(shared_secret, ack_nonce + node_id, ack_hmac)`.
3. If verification fails, return `WireError::HandshakeFailed`.

### 9.4 Security Properties

| Property | How It Is Achieved |
|----------|-------------------|
| **Mutual authentication** | Both sides prove knowledge of the shared secret |
| **Replay protection** | Random UUID nonces per handshake |
| **Timing-attack resistance** | `subtle::ConstantTimeEq` for HMAC comparison |
| **Mandatory secret** | OFP refuses to start with an empty `shared_secret` |
| **Message size limit** | `MAX_MESSAGE_SIZE = 16 MB` prevents memory DoS |
| **Protocol version check** | `PROTOCOL_VERSION` mismatch returns `WireError::VersionMismatch` |

---

## 10. Security Headers

**Source:** `openfang-api/src/middleware.rs`

The `security_headers` middleware is applied to **all** API responses:

```rust
pub async fn security_headers(request: Request<Body>, next: Next) -> Response<Body> {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert("x-xss-protection", "1; mode=block".parse().unwrap());
    headers.insert("content-security-policy", /* CSP policy */);
    headers.insert("referrer-policy", "strict-origin-when-cross-origin".parse().unwrap());
    headers.insert("cache-control", "no-store, no-cache, must-revalidate".parse().unwrap());
    response
}
```

| Header | Value | Protects Against |
|--------|-------|------------------|
| `X-Content-Type-Options` | `nosniff` | MIME type sniffing attacks |
| `X-Frame-Options` | `DENY` | Clickjacking via iframes |
| `X-XSS-Protection` | `1; mode=block` | Reflected XSS (legacy browsers) |
| `Content-Security-Policy` | See below | XSS, code injection, data exfiltration |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Referrer leakage |
| `Cache-Control` | `no-store, no-cache, must-revalidate` | Sensitive data caching |

### 10.1 CSP Breakdown

| Directive | Value | Purpose |
|-----------|-------|---------|
| `default-src` | `'self'` | Deny all external resources by default |
| `script-src` | `'self' 'unsafe-inline' 'unsafe-eval' cdn.jsdelivr.net` | Allow scripts from self and CDN |
| `style-src` | `'self' 'unsafe-inline' cdn.jsdelivr.net fonts.googleapis.com` | Allow styles from self, CDN, Google Fonts |
| `img-src` | `'self' data:` | Allow images from self and data URIs |
| `connect-src` | `'self' ws: wss:` | Allow WebSocket connections |
| `font-src` | `'self' cdn.jsdelivr.net fonts.gstatic.com` | Allow fonts from CDN |
| `object-src` | `'none'` | Block all plugins (Flash, Java, etc.) |
| `base-uri` | `'self'` | Prevent base tag hijacking |
| `form-action` | `'self'` | Restrict form submission targets |

---

## 11. GCRA Rate Limiter

**Source:** `openfang-api/src/rate_limiter.rs`

OpenFang uses the Generic Cell Rate Algorithm (GCRA) for cost-aware API
rate limiting via the `governor` crate.

### 11.1 Algorithm

GCRA is a leaky-bucket variant that tracks a single "virtual scheduling time"
(TAT -- Theoretical Arrival Time) per key.  Each request consumes a number of
tokens proportional to its cost.  The bucket refills at a constant rate.

**Budget:** 500 tokens per minute per IP address.

```rust
pub fn create_rate_limiter() -> Arc<KeyedRateLimiter> {
    Arc::new(RateLimiter::keyed(Quota::per_minute(NonZeroU32::new(500).unwrap())))
}
```

### 11.2 Operation Costs

Each API operation has a configurable token cost:

```rust
pub fn operation_cost(method: &str, path: &str) -> NonZeroU32 {
    match (method, path) {
        (_, "/api/health")                            => 1,
        ("GET", "/api/status")                        => 1,
        ("GET", "/api/version")                       => 1,
        ("GET", "/api/tools")                         => 1,
        ("GET", "/api/agents")                        => 2,
        ("GET", "/api/skills")                        => 2,
        ("GET", "/api/peers")                         => 2,
        ("GET", "/api/config")                        => 2,
        ("GET", "/api/usage")                         => 3,
        ("GET", p) if p.starts_with("/api/audit")     => 5,
        ("GET", p) if p.starts_with("/api/marketplace")=> 10,
        ("POST", "/api/agents")                       => 50,
        ("POST", p) if p.contains("/message")         => 30,
        ("POST", p) if p.contains("/run")             => 100,
        ("POST", "/api/skills/install")               => 50,
        ("POST", "/api/skills/uninstall")             => 10,
        ("POST", "/api/migrate")                      => 100,
        ("PUT", p) if p.contains("/update")           => 10,
        _                                             => 5,
    }
}
```

The cost hierarchy is intentional: read-only health checks cost 1 token while
expensive operations like workflow runs cost 100, meaning a client can perform
500 health checks per minute but only 5 workflow runs.

### 11.3 Middleware

```rust
pub async fn gcra_rate_limit(
    State(limiter): State<Arc<KeyedRateLimiter>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let ip = /* extract from ConnectInfo, default 127.0.0.1 */;
    let cost = operation_cost(&method, &path);

    if limiter.check_key_n(&ip, cost).is_err() {
        tracing::warn!(ip, cost, path, "GCRA rate limit exceeded");
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("retry-after", "60")
            .body(/* JSON error */)
            .unwrap_or_default();
    }
    next.run(request).await
}
```

### 11.4 Rate Limiter Type

```rust
pub type KeyedRateLimiter = RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>;
```

The `DashMapStateStore` provides concurrent per-IP state with automatic stale
entry cleanup.

---

## 12. Path Traversal Prevention

**Source:** `openfang-runtime/src/host_functions.rs`

Two functions provide defense-in-depth against directory traversal.

### 12.1 safe_resolve_path (for reads)

Used for `fs_read` and `fs_list` operations where the target file must exist:

```rust
fn safe_resolve_path(path: &str) -> Result<std::path::PathBuf, serde_json::Value> {
    let p = Path::new(path);

    // Phase 1: Reject any path with ".." components
    for component in p.components() {
        if matches!(component, Component::ParentDir) {
            return Err(json!({"error": "Path traversal denied: '..' components forbidden"}));
        }
    }

    // Phase 2: Canonicalize to resolve symlinks and normalize
    std::fs::canonicalize(p)
        .map_err(|e| json!({"error": format!("Cannot resolve path: {e}")}))
}
```

### 12.2 safe_resolve_parent (for writes)

Used for `fs_write` operations where the target file may not exist yet:

```rust
fn safe_resolve_parent(path: &str) -> Result<std::path::PathBuf, serde_json::Value> {
    let p = Path::new(path);

    // Phase 1: Reject ".." in any component
    for component in p.components() {
        if matches!(component, Component::ParentDir) {
            return Err(json!({"error": "Path traversal denied: '..' components forbidden"}));
        }
    }

    // Phase 2: Canonicalize the parent directory
    let parent = p.parent().filter(|par| !par.as_os_str().is_empty())
        .ok_or_else(|| json!({"error": "Invalid path: no parent directory"}))?;
    let canonical_parent = std::fs::canonicalize(parent)?;

    // Phase 3: Belt-and-suspenders check on filename
    let file_name = p.file_name()
        .ok_or_else(|| json!({"error": "Invalid path: no file name"}))?;
    if file_name.to_string_lossy().contains("..") {
        return Err(json!({"error": "Path traversal denied in file name"}));
    }

    Ok(canonical_parent.join(file_name))
}
```

### 12.3 Enforcement Order

1. **Capability check** runs first with the raw path.
2. **Path traversal check** runs second.
3. **Operation** runs only if both pass.

This ordering ensures that even if a capability is misconfigured with a broad
pattern like `"*"`, path traversal is still blocked.

---

## 13. Subprocess Sandbox

**Source:** `openfang-runtime/src/subprocess_sandbox.rs`

When the runtime spawns child processes (e.g., for the shell tool or skill
execution), the inherited environment must be stripped to prevent accidental
leakage of secrets.

### 13.1 Environment Clearing

```rust
pub fn sandbox_command(cmd: &mut tokio::process::Command, allowed_env_vars: &[String]) {
    cmd.env_clear();  // Remove ALL inherited env vars

    // Re-add platform-independent safe vars
    for var in SAFE_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    // Re-add Windows-specific safe vars (on Windows)
    #[cfg(windows)]
    for var in SAFE_ENV_VARS_WINDOWS { /* ... */ }

    // Re-add caller-specified allowed vars
    for var in allowed_env_vars { /* ... */ }
}
```

### 13.2 Safe Environment Variables

**All platforms:**

```rust
pub const SAFE_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "LANG", "LC_ALL", "TERM",
];
```

**Windows-only:**

```rust
pub const SAFE_ENV_VARS_WINDOWS: &[&str] = &[
    "USERPROFILE", "SYSTEMROOT", "APPDATA", "LOCALAPPDATA",
    "COMSPEC", "WINDIR", "PATHEXT",
];
```

Variables not in these lists and not in `allowed_env_vars` are **never**
passed to the child process.  This means `OPENAI_API_KEY`, `GEMINI_API_KEY`,
database credentials, and all other secrets are stripped.

### 13.3 Executable Path Validation

```rust
pub fn validate_executable_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(format!(
                "executable path '{}' contains '..' component which is not allowed",
                path
            ));
        }
    }
    Ok(())
}
```

This prevents an agent from escaping its working directory via crafted paths
like `../../bin/dangerous`.

### 13.4 Shell Injection Prevention

The `host_shell_exec` function uses `Command::new(command).args(&args)` which
does **not** invoke a shell.  Each argument is passed directly to the
process, preventing shell injection via metacharacters like `;`, `|`, `&&`.

---

## 14. Prompt Injection Scanner

**Source:** `openfang-skills/src/verify.rs`

The `SkillVerifier` provides two scanning functions: `security_scan()` for
skill manifests and `scan_prompt_content()` for skill prompt text (SKILL.md
body).

### 14.1 Manifest Security Scan

`SkillVerifier::security_scan(manifest)` inspects a skill's declared
requirements:

| Check | Severity | Trigger |
|-------|----------|---------|
| Node.js runtime | Warning | `runtime_type == SkillRuntime::Node` |
| Shell execution capability | Critical | Capability contains `shellexec` or `shell_exec` |
| Unrestricted network | Warning | Capability contains `netconnect(*)` |
| Shell tool | Critical | Tool is `shell_exec` or `bash` |
| Filesystem write tool | Warning | Tool is `file_write` or `file_delete` |
| Too many tools | Info | More than 10 tools required |

### 14.2 Prompt Injection Scan

`SkillVerifier::scan_prompt_content(content)` detects common attack patterns
in skill prompt text:

**Critical -- Prompt override attempts:**

```
"ignore previous instructions", "ignore all previous",
"disregard previous", "forget your instructions",
"you are now", "new instructions:", "system prompt override",
"ignore the above", "do not follow", "override system"
```

**Warning -- Data exfiltration patterns:**

```
"send to http", "send to https", "post to http", "post to https",
"exfiltrate", "forward all", "send all data",
"base64 encode and send", "upload to"
```

**Warning -- Shell command references:**

```
"rm -rf", "chmod ", "sudo "
```

**Info -- Excessive length:**

Content over 50,000 bytes triggers an info-level warning about potential LLM
performance degradation.

### 14.3 SHA256 Checksum Verification

```rust
pub fn verify_checksum(data: &[u8], expected_sha256: &str) -> bool {
    let actual = Self::sha256_hex(data);
    actual == expected_sha256.to_lowercase()
}
```

Skills installed from ClawHub have their content verified against a known
SHA256 hash to detect tampering during download.

### 14.4 Warning Structure

```rust
pub struct SkillWarning {
    pub severity: WarningSeverity,  // Info, Warning, Critical
    pub message: String,
}
```

---

## 15. Loop Guard

**Source:** `openfang-runtime/src/loop_guard.rs`

The `LoopGuard` tracks tool calls within a single agent loop execution to
detect when the agent is stuck calling the same tool repeatedly.

### 15.1 Configuration

```rust
pub struct LoopGuardConfig {
    pub warn_threshold: u32,         // Default: 3
    pub block_threshold: u32,        // Default: 5
    pub global_circuit_breaker: u32, // Default: 30
}
```

### 15.2 Detection Algorithm

1. For each tool call, compute SHA-256 of `tool_name + "|" + serialized_params`.
2. Increment the count for that hash in a `HashMap<String, u32>`.
3. Increment `total_calls`.
4. Return a graduated verdict:

```rust
pub fn check(&mut self, tool_name: &str, params: &serde_json::Value) -> LoopGuardVerdict {
    self.total_calls += 1;

    // Global circuit breaker
    if self.total_calls > self.config.global_circuit_breaker {
        return LoopGuardVerdict::CircuitBreak(/* ... */);
    }

    let hash = Self::compute_hash(tool_name, params);
    let count = self.call_counts.entry(hash).or_insert(0);
    *count += 1;

    if *count >= self.config.block_threshold {
        LoopGuardVerdict::Block(/* ... */)
    } else if *count >= self.config.warn_threshold {
        LoopGuardVerdict::Warn(/* ... */)
    } else {
        LoopGuardVerdict::Allow
    }
}
```

### 15.3 Verdict Types

| Verdict | Meaning | Action |
|---------|---------|--------|
| `Allow` | Normal operation | Run the tool |
| `Warn(msg)` | Same call repeated >= 3 times | Run, append warning to result |
| `Block(msg)` | Same call repeated >= 5 times | Skip execution, return error |
| `CircuitBreak(msg)` | > 30 total tool calls | Terminate the entire agent loop |

### 15.4 Hash Computation

```rust
fn compute_hash(tool_name: &str, params: &serde_json::Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool_name.as_bytes());
    hasher.update(b"|");
    let params_str = serde_json::to_string(params).unwrap_or_default();
    hasher.update(params_str.as_bytes());
    hex::encode(hasher.finalize())
}
```

Note: `serde_json::to_string` produces deterministic output (object keys are
sorted), ensuring that semantically identical parameters produce the same hash.

### 15.5 Key Property

Calls with **different parameters** are tracked separately.  An agent that
calls `web_search` with 10 different queries will not trigger the guard, but
an agent that calls `web_search({"query": "test"})` 5 times will be blocked.

---

## 16. Session Repair

**Source:** `openfang-runtime/src/session_repair.rs`

Before sending message history to the LLM, this module validates and repairs
common structural issues that would cause API errors.

### 16.1 Three-Phase Repair

```rust
pub fn validate_and_repair(messages: &[Message]) -> Vec<Message>
```

**Phase 1 -- Collect ToolUse IDs:**

Scan all messages for `ContentBlock::ToolUse { id, .. }` blocks and collect
their IDs into a `HashSet<String>`.

**Phase 2 -- Filter orphans and empties:**

- **Orphaned ToolResults:** `ContentBlock::ToolResult { tool_use_id, .. }`
  blocks where `tool_use_id` is not in the ToolUse ID set are dropped.
- **Empty messages:** Messages with empty text or no content blocks are
  dropped.

**Phase 3 -- Merge consecutive same-role messages:**

The Anthropic API requires strict role alternation (user, assistant, user,
assistant...).  If two consecutive messages have the same role, they are
merged into a single message with combined content blocks.

### 16.2 Why Each Repair Is Needed

| Issue | Cause | Effect Without Repair |
|-------|-------|----------------------|
| Orphaned ToolResult | Compaction or truncation removed the ToolUse | API error: "tool_use_id not found" |
| Empty messages | Cancelled generation, empty user submission | API error: empty content |
| Consecutive same-role | Manual history editing, session repair itself | API error: role alternation violation |

### 16.3 Content Merging

When merging consecutive same-role messages, both are converted to block
format and concatenated:

```rust
fn merge_content(dst: &mut MessageContent, src: MessageContent) {
    let dst_blocks = content_to_blocks(std::mem::replace(dst, MessageContent::Text(String::new())));
    let src_blocks = content_to_blocks(src);
    let mut combined = dst_blocks;
    combined.extend(src_blocks);
    *dst = MessageContent::Blocks(combined);
}
```

---

## 17. Health Endpoint Redaction

**Source:** `openfang-api/src/routes.rs`

OpenFang provides two health endpoints with different information levels.

### 17.1 Public Endpoint: `GET /api/health`

**No authentication required.**  Returns only liveness information:

```json
{
    "status": "ok",
    "version": "0.1.0"
}
```

This endpoint does not expose agent count, database details, configuration
warnings, uptime, or any internal system information.  It is suitable for
load balancer health checks.

### 17.2 Detail Endpoint: `GET /api/health/detail`

**Requires authentication.**  Returns full diagnostics:

```json
{
    "status": "ok",
    "version": "0.1.0",
    "uptime_seconds": 3600,
    "panic_count": 0,
    "restart_count": 2,
    "agent_count": 15,
    "database": "connected",
    "config_warnings": []
}
```

### 17.3 Localhost Fallback

When no API key is configured, the `auth` middleware restricts all
non-health endpoints to loopback addresses only:

```rust
if api_key.is_empty() {
    let is_loopback = request.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().is_loopback())
        .unwrap_or(false);
    if !is_loopback {
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(/* "No API key configured. Remote access denied." */)
            ...;
    }
}
```

---

## 18. Security Configuration

### 18.1 config.toml Reference

```toml
# API Authentication
api_key = "your-secret-api-key"  # Empty = localhost-only mode

# OFP Wire Protocol
[network]
shared_secret = "your-pre-shared-key"  # Required for OFP

# WASM Sandbox
[sandbox]
fuel_limit = 1000000       # CPU instruction budget per execution
timeout_secs = 30          # Wall-clock timeout per execution
max_memory_bytes = 16777216 # 16 MB max WASM memory

# Rate Limiting
# 500 tokens/minute/IP (not currently configurable via config.toml)

# Web Search SSRF Protection
[web]
# SSRF protection is always on and cannot be disabled
```

### 18.2 Environment Variables for Secrets

| Variable | Used By |
|----------|---------|
| `OPENAI_API_KEY` | OpenAI-compat driver |
| `ANTHROPIC_API_KEY` | Anthropic driver |
| `GEMINI_API_KEY` or `GOOGLE_API_KEY` | Gemini driver |
| `DEEPSEEK_API_KEY` | DeepSeek provider |
| `GROQ_API_KEY` | Groq provider |
| `BRAVE_API_KEY` | Brave web search |
| `TAVILY_API_KEY` | Tavily web search |
| `PERPLEXITY_API_KEY` | Perplexity web search |

All environment variable API keys are wrapped in `Zeroizing<String>` when
loaded into driver structs.

### 18.3 Capability Declaration (Agent Manifest)

Capabilities are declared in the agent's TOML manifest:

```toml
[agent]
name = "my-agent"

[[capabilities]]
type = "FileRead"
value = "/data/*"

[[capabilities]]
type = "NetConnect"
value = "*.openai.com:443"

[[capabilities]]
type = "ToolInvoke"
value = "web_search"

[[capabilities]]
type = "LlmMaxTokens"
value = 4096
```

### 18.4 Loop Guard Tuning

The default `LoopGuardConfig` values are:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `warn_threshold` | 3 | Identical calls before warning |
| `block_threshold` | 5 | Identical calls before blocking |
| `global_circuit_breaker` | 30 | Total calls before circuit break |

### 18.5 Subprocess Sandbox Allowlists

To pass specific environment variables to subprocesses:

```rust
sandbox_command(&mut cmd, &["MY_CUSTOM_VAR".to_string()]);
```

Only variables explicitly listed in `allowed_env_vars` (plus the safe
defaults) will be inherited.

---

## 19. Security Dependencies

| Crate | Purpose |
|-------|---------|
| `sha2` | SHA-256 hashing (audit trail, loop guard, SSRF, checksums) |
| `hmac` | HMAC-SHA256 for OFP authentication |
| `hex` | Hex encoding/decoding of hashes and signatures |
| `subtle` | Constant-time comparison (`ConstantTimeEq`) for HMAC verification |
| `ed25519-dalek` | Ed25519 signing/verification for manifest signing |
| `rand` | Cryptographic RNG for key generation (`OsRng`) |
| `zeroize` | `Zeroizing<T>` wrapper for automatic secret memory wiping |
| `governor` | GCRA rate limiting algorithm |
| `wasmtime` | WASM sandbox with fuel + epoch metering |
| `uuid` | Nonce generation for OFP handshakes |
| `chrono` | ISO-8601 timestamps for audit entries |
| `reqwest` | HTTP client (used inside SSRF-protected `host_net_fetch`) |

### 19.1 Why These Specific Crates

- **sha2/hmac:** Part of the RustCrypto project, audited, widely used in production Rust.
- **ed25519-dalek:** De facto standard Ed25519 library in Rust, extensively audited.
- **subtle:** Provides constant-time operations to prevent timing side-channels.
- **zeroize:** Official RustCrypto approach to zeroing secrets; integrates with `Drop`.
- **governor:** Battle-tested GCRA implementation with `DashMap`-backed concurrent state.

---

## Threat Model Summary

| Threat | Mitigated By |
|--------|-------------|
| Agent requests unauthorized file access | Capability-based security (Section 2) |
| Agent spawns child with elevated privileges | Capability inheritance validation (Section 2.4) |
| WASM skill runs infinite loop | Dual metering: fuel + epoch (Section 3) |
| Attacker tampers with audit log | Merkle hash chain (Section 4) |
| Prompt injection via external data | Taint tracking (Section 5) |
| Data exfiltration via LLM | Taint sinks block Secret/PII to net_fetch (Section 5.3) |
| Tampered agent manifest | Ed25519 signing (Section 6) |
| SSRF to cloud metadata | Private IP + hostname blocking + DNS check (Section 7) |
| API key recovery from memory dump | Zeroizing<String> (Section 8) |
| Unauthorized peer-to-peer connections | HMAC-SHA256 mutual auth (Section 9) |
| XSS / clickjacking on API | Security headers (Section 10) |
| API brute force / DoS | GCRA rate limiter (Section 11) |
| Path traversal via `../` | safe_resolve_path / safe_resolve_parent (Section 12) |
| Secret leakage to child processes | env_clear() + allowlist (Section 13) |
| Malicious skills from ClawHub | Prompt injection scanner + SHA256 checksum (Section 14) |
| Agent stuck in tool loop | LoopGuard with graduated response (Section 15) |
| Corrupted LLM session history | Session repair (Section 16) |
| Information leakage from health endpoint | Redacted public endpoint (Section 17) |
| Timing attacks on HMAC verification | subtle::ConstantTimeEq (Section 9.2) |
| Shell injection via metacharacters | Command::new (no shell) + env_clear (Section 13.4) |
| DNS rebinding for SSRF bypass | Resolved IP check, not hostname check (Section 7.3) |
