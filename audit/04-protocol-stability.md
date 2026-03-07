# Dimension 4 — Protocol & API Stability

**Auditor:** Copilot  
**Date:** 2025-07-15  
**Scope:** Core types serde contracts, wire protocol (OFP) versioning, enum representations, backward compatibility, API request/response stability

---

## Summary

OpenFang demonstrates **strong protocol design discipline** across its type system. The vast majority of enums use internally-tagged representations (`#[serde(tag = "type")]`, `#[serde(tag = "event")]`, `#[serde(tag = "kind")]`), which are the most forward-compatible serde strategy. Structs consistently use `#[serde(default)]` at the container level, enabling new fields to be added without breaking existing payloads. A dedicated `serde_compat` module provides lenient deserialization (`vec_lenient`, `map_lenient`) that gracefully handles schema evolution in stored msgpack data. The wire protocol (OFP) includes a `PROTOCOL_VERSION` constant and transmits it during handshakes. However, there are a few notable gaps: two `#[serde(untagged)]` enums that risk silent misparse, no `deny_unknown_fields` anywhere (tolerable but intentional), and no version field in the core `Event` or `AgentManifest` types for on-wire evolution.

**Overall Grade: A-** — Mature serde practices with excellent backward-compat infrastructure; minor risks from untagged enums and missing schema version fields.

---

## Findings

### P1 — `MessageContent` Uses `#[serde(untagged)]` — Silent Misparse Risk
**Severity:** High  
**Location:** `crates\openfang-types\src\message.rs:28`

```rust
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}
```

Untagged enums in serde attempt each variant in order and silently pick the first match. If a `Blocks` payload happens to be valid as a `String` (it won't for `Vec`, but future variants could), deserialization silently picks the wrong variant. More importantly, when deserialization of `Blocks` fails for any reason, serde falls back to `Text` with a confusing error message rather than reporting the actual problem. This is a known serde anti-pattern for evolving schemas.

**Recommendation:** Use an internally-tagged representation or an adjacently-tagged enum:
```rust
#[serde(tag = "type", content = "value")]
pub enum MessageContent {
    #[serde(rename = "text")]
    Text(String),
    #[serde(rename = "blocks")]
    Blocks(Vec<ContentBlock>),
}
```
If wire compatibility with OpenAI-style payloads requires untagged, document this trade-off explicitly and add exhaustive deserialization tests for every variant, including malformed inputs.

---

### P2 — `OaiContent` Uses `#[serde(untagged)]` with `#[default]` — Triple Fallback Hazard
**Severity:** High  
**Location:** `crates\openfang-api\src\openai_compat.rs:43-49`

```rust
#[serde(untagged)]
pub enum OaiContent {
    Text(String),
    Parts(Vec<OaiContentPart>),
    #[default]
    Null,
}
```

This combines two risky patterns: `untagged` (silent misparse) and `#[default]` (any failed deserialization yields `Null`). If a client sends a malformed `Parts` array, it silently becomes `Null` instead of returning an error. This is acceptable for OpenAI API compatibility (the OpenAI spec uses this exact pattern), but it should be documented as an intentional compatibility concession, not treated as a general pattern.

**Recommendation:** Add a code comment marking this as an intentional OpenAI wire-compat concession. Do NOT propagate this pattern to OpenFang-native types. Consider adding validation in the handler that rejects `Null` content when the user clearly intended to send something.

---

### P3 — No Schema Version in `Event` or `AgentManifest` Envelopes
**Severity:** Medium  
**Location:** `crates\openfang-types\src\event.rs:283`, `crates\openfang-types\src\agent.rs:416`

The `Event` struct has no version field. While `#[serde(default)]` on `AgentManifest` handles additive changes gracefully, there is no mechanism for breaking changes. If a field type changes (e.g., `resources` goes from flat struct to nested), old serialized events/manifests become unparseable despite the lenient deserializers.

The `AgentManifest` has a `version` field, but it represents the agent's semantic version, not the manifest schema version.

**Recommendation:** Add a `schema_version: u32` field (defaulting to 1) to both `Event` and `AgentManifest`. This enables future migrations:
```rust
#[serde(default = "schema_v1")]
pub schema_version: u32,
```

---

### P4 — Wire Protocol Version Constant Is Not Enforced in Handshake
**Severity:** Medium  
**Location:** `crates\openfang-wire\src\message.rs:152`, `crates\openfang-wire\src\peer.rs`

`PROTOCOL_VERSION` is defined as `1` and transmitted in `Handshake`/`HandshakeAck` messages, which is excellent. However, the peer node implementation should verify that the remote peer's `protocol_version` is compatible (equal or within a supported range). If version checking is missing in the handshake handler, a v2 peer could connect to a v1 peer and silently corrupt messages.

**Recommendation:** Verify `protocol_version` during handshake acceptance. Reject connections from peers with incompatible versions and log a clear error. Consider supporting a version range (`min_version..=max_version`) for rolling upgrades.

---

### P5 — `WireMessage` Uses `#[serde(flatten)]` — Performance and Ambiguity Risk
**Severity:** Low  
**Location:** `crates\openfang-wire\src\message.rs:14-16`

```rust
pub struct WireMessage {
    pub id: String,
    #[serde(flatten)]
    pub kind: WireMessageKind,
}
```

`#[serde(flatten)]` causes serde to buffer the entire JSON object into a temporary `Map` before dispatching, which has both performance and correctness implications: (1) it's ~2x slower than non-flattened deserialization, and (2) duplicate keys between `WireMessage` fields and `WireMessageKind` fields would silently shadow each other. For a wire protocol handling high-throughput message streams, this overhead is meaningful.

**Recommendation:** This is a deliberate design trade-off for a cleaner wire format (`{"id": "...", "type": "request", ...}` vs `{"id": "...", "kind": {"type": "request", ...}}`). Acceptable for now, but if OFP throughput becomes critical, benchmark both approaches and consider nested encoding.

---

### P6 — No `deny_unknown_fields` on Any Type
**Severity:** Low  
**Location:** All types across `openfang-types`, `openfang-wire`, `openfang-api`

No struct or enum uses `#[serde(deny_unknown_fields)]`. This means typos in JSON field names are silently ignored. For example, sending `{"manifst_toml": "..."}` to the spawn endpoint would silently use defaults instead of the intended manifest.

**Recommendation:** This is a deliberate forward-compatibility choice — `deny_unknown_fields` prevents clients from sending fields the server doesn't yet know about, which breaks rolling upgrades. The current approach is correct for an evolving system. Consider adding `deny_unknown_fields` selectively to security-critical types like `ApprovalResponse` and `SpawnRequest` where silent field dropping could mask misconfiguration.

---

### P7 — `Capability` Enum Has No Catch-All Variant
**Severity:** Low  
**Location:** `crates\openfang-types\src\capability.rs:10-72`

```rust
#[serde(tag = "type", content = "value")]
pub enum Capability {
    FileRead(String),
    // ... 18 more variants ...
    EconTransfer(String),
}
```

Unlike `ContentBlock` which has `#[serde(other)] Unknown`, the `Capability` enum has no catch-all. Adding a new capability variant (e.g., `DatabaseQuery`) would cause deserialization failures for any stored manifests or messages containing the new variant. Since manifests are persisted in SQLite as msgpack, this is a real upgrade hazard.

**Recommendation:** Add an `Unknown` catch-all variant:
```rust
#[serde(other)]
Unknown,
```
Note: `#[serde(other)]` only works with `#[serde(tag = "...")]` enums when the variant is unit-like. Since `Capability` uses `tag + content`, consider an alternative like wrapping unknown variants in `Other(serde_json::Value)`.

---

### P8 — `ScheduleMode` Enum Variants Mix Unit and Struct Forms
**Severity:** Low  
**Location:** `crates\openfang-types\src\agent.rs:219-234`

```rust
#[serde(rename_all = "snake_case")]
pub enum ScheduleMode {
    Reactive,
    Periodic { cron: String },
    Proactive { conditions: Vec<String> },
    Continuous { check_interval_secs: u64 },
}
```

This enum uses `rename_all` but no explicit `#[serde(tag = "...")]`, so it defaults to serde's externally-tagged representation (`{"Periodic": {"cron": "..."}}`). This is inconsistent with the rest of the codebase which overwhelmingly uses internally-tagged enums (`#[serde(tag = "type")]`). The externally-tagged form is less extensible — adding new fields to an existing variant changes the nesting structure.

**Recommendation:** Add `#[serde(tag = "kind")]` for consistency and better evolution:
```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleMode { ... }
```
Note: This is a breaking serialization change — requires migration of any persisted data.

---

### P9 — `Priority` Enum Uses Integer Discriminants — Fragile Ordering
**Severity:** Low  
**Location:** `crates\openfang-types\src\agent.rs:278-289`

```rust
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}
```

The enum has explicit integer discriminants and derives `PartialOrd, Ord` via the enum order. It also derives `Serialize, Deserialize` without `#[serde(rename_all)]`, so it serializes as `"Low"`, `"Normal"`, etc. (PascalCase). This is inconsistent with every other enum in the codebase which uses `snake_case` or `lowercase` rename strategies. A JSON payload with `"priority": "Low"` looks foreign next to `"risk_level": "low"`.

**Recommendation:** Add `#[serde(rename_all = "snake_case")]` for consistency. This is a minor wire-format change.

---

### P10 — `ExportFormat` Enum Lacks Rename Strategy
**Severity:** Low  
**Location:** `crates\openfang-types\src\memory.rs:237-243`

```rust
pub enum ExportFormat {
    Json,
    MessagePack,
}
```

Serializes as `"Json"` and `"MessagePack"` (PascalCase) rather than the codebase-standard `"json"` / `"message_pack"`. Minor inconsistency.

**Recommendation:** Add `#[serde(rename_all = "snake_case")]`.

---

## Strengths

### S1 — Consistent Use of Internally-Tagged Enums
The codebase overwhelmingly uses `#[serde(tag = "type")]`, `#[serde(tag = "event")]`, `#[serde(tag = "kind")]`, and `#[serde(tag = "method")]` for enum serialization. This is the gold standard for evolvable JSON protocols — each variant self-identifies via a discriminator field, enabling forward-compatible deserialization and clear error messages when an unknown variant is encountered.

### S2 — Pervasive `#[serde(default)]` on Structs
Nearly every configuration and manifest struct uses container-level `#[serde(default)]` with explicit `Default` implementations. This means new fields can be added freely without breaking existing serialized data — existing payloads simply get the default values for missing fields. This is textbook backward compatibility.

### S3 — Dedicated `serde_compat` Module with Lenient Deserializers
The `serde_compat` module (`vec_lenient`, `map_lenient`) is a sophisticated solution to the schema evolution problem for stored msgpack data. It gracefully handles type changes (e.g., a field that was `u64` becomes `Vec<String>`) by returning empty defaults instead of hard errors. This is applied to critical manifest fields like `fallback_models`, `skills`, `tools`, `tags`, and `capabilities` — the exact fields most likely to evolve. Thorough tests validate all type-mismatch scenarios including maps, integers, strings, booleans, and nulls.

### S4 — `ContentBlock` Has `#[serde(other)] Unknown` Catch-All
The `ContentBlock` enum includes an `Unknown` variant for forward compatibility. When a newer API returns a content block type that the current code doesn't recognize, it gracefully deserializes as `Unknown` instead of failing. This is validated by a test (`test_content_block_unknown_deser`).

### S5 — Wire Protocol Is Well-Structured with Version Constant
The OFP wire protocol uses a clean message hierarchy (`WireMessage` → `WireMessageKind` → `WireRequest`/`WireResponse`/`WireNotification`) with explicit discriminator tags at each level. The `PROTOCOL_VERSION` constant is defined and exchanged during handshakes. Messages use length-prefixed framing (4-byte big-endian + JSON), which is a robust framing strategy.

### S6 — API Types Use Asymmetric Derive (Serialize vs Deserialize)
API request types derive only `Deserialize`, and response types derive only `Serialize`. This is correct — it prevents accidental misuse (e.g., deserializing a response type from user input) and makes the one-way nature of API contracts explicit.

### S7 — Custom Duration Serialization via `serde(with)`
The `Event.ttl` field uses a custom `duration_ms` module to serialize `Option<Duration>` as milliseconds. This is important for cross-language compatibility — `Duration` doesn't have a universal JSON representation, so choosing milliseconds and implementing custom ser/de is the right approach.

### S8 — OpenAI Compatibility Layer Is Cleanly Separated
The OpenAI-compatible types (`OaiContent`, `OaiContentPart`, `ChatCompletionRequest/Response`) are isolated in `openai_compat.rs` with their own serde conventions (matching OpenAI's API spec). The untagged enum usage is constrained to this compatibility layer and doesn't leak into core OpenFang types.

### S9 — Handshake Auth Fields Default Gracefully
The wire protocol handshake includes `nonce` and `auth_hmac` fields with `#[serde(default)]`, allowing unauthenticated peers to connect without sending these fields. This enables graceful upgrade from unauthenticated to authenticated OFP connections.

### S10 — `SignedManifest` Provides Supply Chain Integrity
The `manifest_signing` module implements Ed25519 manifest signing with SHA-256 content hashing. The `SignedManifest` struct is cleanly serializable and the verify/sign flow is well-tested. This prevents privilege escalation via tampered manifests.
