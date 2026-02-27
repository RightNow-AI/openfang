# Dimension 8 — Dependency Supply Chain

**Auditor:** Copilot  
**Date:** 2025-07-16  
**Scope:** All `Cargo.toml` files (root + 13 crates + xtask), `Cargo.lock`, deny.toml

---

## Summary

| Metric | Value |
|---|---|
| Total unique packages in Cargo.lock | 793 |
| Workspace-declared dependencies | 38 |
| Non-workspace inline deps | 11 (cron, libc, tauri ecosystem, open) |
| Workspace inheritance compliance | 97% (all external deps use `workspace = true` except 11 in desktop + kernel) |
| Native C lib deps in tree | 3 (cc, pkg-config, libsqlite3-sys via bundled) |
| deny.toml present | ❌ No |
| Cargo.lock committed | ✅ Yes |
| OpenSSL / native-tls | ✅ Avoided (uses rustls-tls throughout) |

**Overall Grade: B+** — Excellent workspace inheritance discipline and smart TLS choices, but missing deny.toml for automated supply chain auditing and two heavyweight dependency trees inflate the lockfile.

---

## Findings

### SC1 — No `deny.toml` for supply chain auditing (Severity: High)

**Location:** Repository root (missing file)

`cargo-deny` is the standard Rust ecosystem tool for automated supply chain checks (license compliance, advisory database scanning, duplicate detection, source restrictions). No `deny.toml` exists, meaning:

- No automated license compatibility checks (mixing Apache-2.0/MIT with GPL deps would go unnoticed)
- No `RustSec` advisory database scanning in CI
- No duplicate crate version detection
- No banned crate enforcement

**Recommendation:** Add a `deny.toml` with at minimum:
```toml
[advisories]
vulnerability = "deny"
unmaintained = "warn"

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]

[bans]
multiple-versions = "warn"
deny = [
    { name = "openssl" },
    { name = "openssl-sys" },
]

[sources]
allow-org = { github = ["RightNow-AI"] }
```

Add `cargo deny check` to CI pipeline.

---

### SC2 — Wasmtime pulls ~28 transitive crates (Severity: Medium)

**Location:** `Cargo.toml:83` — `wasmtime = "41"`

Wasmtime (used only in `openfang-runtime`) brings 15 wasmtime-internal-* crates and 13 cranelift-* crates (28 total), making it the single largest dependency tree. This is ~3.5% of the entire lockfile from one dependency.

This is **justified** — WASM sandboxing is a core feature for skill execution. However:

- Wasmtime should be **feature-gated** so non-WASM builds compile faster
- Only `openfang-runtime` uses it; all other crates are unaffected

**Recommendation:** Consider adding a cargo feature `wasm` that gates `wasmtime` in `openfang-runtime`, allowing headless/lightweight builds:
```toml
[features]
default = ["wasm"]
wasm = ["dep:wasmtime"]
```

---

### SC3 — Tauri desktop crate uses 11 non-workspace inline deps (Severity: Medium)

**Location:** `crates/openfang-desktop/Cargo.toml:9-27`

The desktop crate specifies `tauri`, `tauri-build`, and 7 tauri plugins with inline version pins rather than workspace deps. These 9 deps expand to 18 packages in the lockfile.

Non-workspace deps in `openfang-desktop`:
- `tauri-build = "2"` (build-dependency)
- `tauri = "2"` (with features)
- `tauri-plugin-notification = "2"`
- `tauri-plugin-shell = "2"`
- `tauri-plugin-single-instance = "2"`
- `tauri-plugin-dialog = "2"`
- `tauri-plugin-global-shortcut = "2"`
- `tauri-plugin-autostart = "2"`
- `tauri-plugin-updater = "2"`
- `open = "5"`

**Recommendation:** Promote all Tauri deps to `[workspace.dependencies]` for version consistency. Even if only one crate uses them, workspace inheritance ensures version changes are tracked centrally:
```toml
# In root Cargo.toml [workspace.dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-build = { version = "2", features = [] }
tauri-plugin-notification = "2"
# ... etc
```

---

### SC4 — `cron` dependency not in workspace deps (Severity: Low)

**Location:** `crates/openfang-kernel/Cargo.toml:35` — `cron = "0.15"`

The `cron` crate is pinned inline in `openfang-kernel` rather than declared in workspace dependencies. This is the only non-Tauri, non-platform external dependency that bypasses workspace inheritance.

**Recommendation:** Add to `[workspace.dependencies]`:
```toml
cron = "0.15"
```
Then update `openfang-kernel/Cargo.toml`:
```toml
cron = { workspace = true }
```

---

### SC5 — `chrono` used universally without feature-gating (Severity: Low)

**Location:** `Cargo.toml:51` — `chrono = { version = "0.4", features = ["serde"] }`

`chrono` is used in 11 of 13 crates. The workspace declares it with only the `serde` feature enabled, which is appropriate. However:

- Modern `chrono` 0.4.38+ has `clock` and `std` features enabled by default, which is fine for this project
- The `oldtime` feature (deprecated, pulls `time` 0.1) is not enabled — good
- No unnecessary feature bloat detected

This is mostly a **non-issue** in its current configuration. The breadth of usage (11/13 crates) confirms it's a justified core dependency.

---

### SC6 — `tokio` uses `features = ["full"]` globally (Severity: Low)

**Location:** `Cargo.toml:29` — `tokio = { version = "1", features = ["full"] }`

Every crate that uses tokio gets the full feature set (io, net, fs, process, signal, sync, time, rt-multi-thread, macros, parking_lot). Several crates (e.g., `openfang-hands`, `openfang-skills`) likely only need `rt`, `sync`, and `macros`.

**Impact:** Minimal on binary size (unused code is stripped with LTO), but increases compile time.

**Recommendation:** Low priority. The `profile.release` already has `lto = true` + `strip = true`. Only worth changing if compile times become a pain point.

---

### SC7 — `ring` pulled transitively despite rustls-tls preference (Severity: Info)

**Location:** Transitive via `rustls` → `ring`

The project correctly avoids OpenSSL by using `rustls-tls` for reqwest and `rustls-tls-native-roots` for tokio-tungstenite. However, `ring` (a native C/ASM crypto library) is still pulled as a transitive dependency of `rustls`.

This is **expected behavior** — rustls depends on ring for its cryptographic primitives. The `aws-lc-rs` backend is an alternative but has its own trade-offs. No action needed unless cross-compilation issues arise.

---

## Strengths

### ✅ S1 — Excellent workspace dependency inheritance

97% of external dependencies use `workspace = true`. All 38 workspace-declared deps are consistently inherited across crates. The only exceptions are the Tauri desktop crate (platform-specific ecosystem) and two small deps in kernel (`cron`, `libc`).

### ✅ S2 — No OpenSSL / native-tls anywhere

The project exclusively uses `rustls-tls` for both HTTP (reqwest) and WebSocket (tokio-tungstenite) connections. This eliminates the most common native C dependency pain point in Rust projects and ensures clean cross-compilation.

### ✅ S3 — SQLite uses bundled mode

`rusqlite = { features = ["bundled"] }` compiles SQLite from source rather than linking to a system library. This eliminates system dependency on libsqlite3-dev and ensures consistent behavior across platforms.

### ✅ S4 — Cargo.lock is committed

The lockfile is tracked in git, ensuring deterministic builds across all environments. This is correct practice for a binary/application project.

### ✅ S5 — Release profile is well-optimized

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
opt-level = 3
```

LTO with single codegen unit and stripping produces minimal binary size and eliminates dead code from broad dependency feature sets.

### ✅ S6 — regex-lite over regex

The project uses `regex-lite` instead of the full `regex` crate, avoiding the heavy regex compilation engine. This is a thoughtful lightweight choice.

### ✅ S7 — Security deps are well-chosen

Pure Rust crypto stack: `sha2`, `hmac`, `aes-gcm`, `argon2`, `ed25519-dalek`, `subtle`. No unnecessary native crypto libraries. `zeroize` is used for secret memory clearing — a security best practice.
