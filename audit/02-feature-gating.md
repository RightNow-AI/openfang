# Dimension 2: Binary Size & Feature Gating

## Summary

OpenFang has **zero compile-time feature gating** across its 13 production crates — not a single `#[cfg(feature = "...")]` attribute exists in any `.rs` file. Every dependency (including heavyweight crates like `wasmtime` at 15 sub-packages, `tauri` at 18 sub-packages, and the full `tokio` runtime) is unconditionally compiled into every build. The Cargo.lock contains **793 packages**, and the workspace has no `default-members` to exclude desktop-only or optional subsystems, meaning `cargo build --workspace` always compiles everything.

## Findings

### Finding F1: Zero Feature Gating in the Entire Codebase

| Field | Content |
|-------|---------|
| **Issue** | No `#[cfg(feature = "...")]` attributes exist anywhere in 189 Rust source files — all code compiles unconditionally |
| **Evidence** | `grep -r '#[cfg(feature' crates/` returns zero results; only `openfang-desktop/Cargo.toml:33` defines a feature (`custom-protocol`) for Tauri internals |
| **Risk Tier** | High |
| **Impact** | Users who only need the CLI daemon still compile wasmtime (WASM sandbox), ratatui (TUI), all channel bridges (Discord/Slack/Telegram/Matrix), encryption vault (aes-gcm/argon2), and every other subsystem. Binary size is maximally bloated; compile times are unnecessarily long. |
| **Direction** | Define feature flags for major subsystems: `wasm-sandbox`, `channels`, `tui`, `encryption-vault`, `desktop`. Gate code with `#[cfg(feature = "...")]` and make heavy deps `optional = true`. Provide a `full` meta-feature that enables everything. |

### Finding F2: Wasmtime Unconditionally Compiled into CLI Binary

| Field | Content |
|-------|---------|
| **Issue** | `wasmtime` (15 sub-packages, heavyweight WASM runtime) is always compiled, but only used in a single file |
| **Evidence** | `crates/openfang-runtime/Cargo.toml:25` — `wasmtime = { workspace = true }`; only used in `crates/openfang-runtime/src/sandbox.rs`. The CLI binary (`openfang-cli`) transitively pulls this in via `openfang-runtime`. |
| **Risk Tier** | High |
| **Impact** | wasmtime is one of the largest Rust dependencies in existence. It adds significant compile time (minutes) and binary size (megabytes) for a feature most users may never use. Serverless or embedded deployments pay this cost with no benefit. |
| **Direction** | Make `wasmtime` an optional dependency gated behind a `wasm-sandbox` feature in `openfang-runtime`. Guard `sandbox.rs` with `#[cfg(feature = "wasm-sandbox")]` and provide a stub/error when disabled. |

### Finding F3: No Workspace `default-members` — Desktop Always Compiled

| Field | Content |
|-------|---------|
| **Issue** | `openfang-desktop` (Tauri, 18 sub-packages) is a workspace member with no exclusion mechanism |
| **Evidence** | `Cargo.toml:3-18` lists all 14 crates in `members` with no `default-members` override. CI files run `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace` with no `--exclude`. |
| **Risk Tier** | Medium |
| **Impact** | Every `cargo build --workspace` or `cargo test --workspace` compiles Tauri and all its native GUI dependencies, even on headless servers or CI runners that will never run the desktop app. This adds ~18 packages plus their transitive deps to compile time. |
| **Direction** | Add `default-members` to the workspace that excludes `openfang-desktop`. Developers building the desktop app can use `cargo build -p openfang-desktop`. CI can have a separate job for desktop builds. |

### Finding F4: Channel Bridges Are Monolithic — No Per-Channel Features

| Field | Content |
|-------|---------|
| **Issue** | All channel integrations (Discord, Slack, Telegram, Matrix, etc.) compile unconditionally with no way to select specific channels |
| **Evidence** | `crates/openfang-channels/Cargo.toml` — all deps (tokio-tungstenite, reqwest, axum, hmac, sha2, etc.) are unconditional. No `[features]` section exists. `openfang-api/Cargo.toml:13` unconditionally depends on openfang-channels. |
| **Risk Tier** | Medium |
| **Impact** | A deployment that only uses Slack still compiles Discord's WebSocket gateway code, Telegram's bot API client, and Matrix protocol handling. Each channel brings WebSocket, HTTP, and crypto dependencies. |
| **Direction** | Define per-channel features: `discord`, `slack`, `telegram`, `matrix`. Gate each channel module and its specific deps behind its feature. Provide `channels-all` meta-feature. |

### Finding F5: `crossbeam` Dependency Appears Unused in openfang-kernel

| Field | Content |
|-------|---------|
| **Issue** | `crossbeam` is listed as a dependency but no code imports or uses it |
| **Evidence** | `crates/openfang-kernel/Cargo.toml:22` — `crossbeam = { workspace = true }`. Searching for `use crossbeam` or `crossbeam::` in `crates/openfang-kernel/src/` returns zero results. |
| **Risk Tier** | Low |
| **Impact** | Unnecessary dependency adds to compile time and binary size. `crossbeam` itself is relatively lightweight but its inclusion suggests dead code or a forgotten refactor. |
| **Direction** | Remove `crossbeam` from `openfang-kernel/Cargo.toml`. If needed in the future, re-add it. Also audit for other potentially unused deps. |

### Finding F6: Non-Workspace Dependencies Break Version Consistency

| Field | Content |
|-------|---------|
| **Issue** | Several crate-level dependencies bypass `[workspace.dependencies]`, preventing centralized version management |
| **Evidence** | `crates/openfang-kernel/Cargo.toml:35` — `cron = "0.15"` (not in workspace). `crates/openfang-kernel/Cargo.toml:38` — `libc = "0.2"` (not in workspace). `crates/openfang-desktop/Cargo.toml:20-30` — `tauri`, 7 `tauri-plugin-*` crates, and `open` all hardcode versions. |
| **Risk Tier** | Low |
| **Impact** | Version bumps require editing individual crate Cargo.toml files instead of the single workspace root. Risk of version drift if these deps are ever used by multiple crates. |
| **Direction** | Add `cron`, `libc`, and `open` to `[workspace.dependencies]` in the root `Cargo.toml`. For Tauri deps, consider adding them to workspace deps as well, or document the intentional exception since they are desktop-only. |

### Finding F7: No Meta-Feature (`full`) or Feature Documentation

| Field | Content |
|-------|---------|
| **Issue** | No crate defines a `full` or `default` feature set, and there is no documentation of available features |
| **Evidence** | Only `openfang-desktop/Cargo.toml:32-34` defines any features at all (`custom-protocol`). No other crate has a `[features]` section. No feature documentation exists in README.md or docs/. |
| **Risk Tier** | Medium |
| **Impact** | When feature flags are eventually added (per F1-F4), there will be no established pattern for meta-features or defaults. Users will need clear guidance on which features to enable for their deployment scenario. |
| **Direction** | When implementing feature gating, establish a `default` feature set for common deployments and a `full` feature that enables everything. Document feature flags in README.md and each crate's Cargo.toml. |

### Finding F8: Heavy Encryption Deps Always Compiled

| Field | Content |
|-------|---------|
| **Issue** | `aes-gcm` and `argon2` (computationally expensive crypto) are unconditional deps in openfang-extensions |
| **Evidence** | `crates/openfang-extensions/Cargo.toml:29-30` — `aes-gcm = { workspace = true }` and `argon2 = { workspace = true }`. Used in `vault.rs` for credential encryption. `openfang-cli` depends on `openfang-extensions`, so every CLI build includes these. |
| **Risk Tier** | Low |
| **Impact** | Deployments that don't use the credential vault still compile Argon2 (password hashing) and AES-256-GCM (symmetric encryption). These are CPU-intensive crates that increase compile time. |
| **Direction** | Gate behind an `encryption-vault` feature in openfang-extensions. The vault module can return a clear error when the feature is disabled. |

## Strengths

- **Workspace dependency inheritance is well-adopted**: 11 of 13 production crates use `{ workspace = true }` for all shared dependencies, ensuring version consistency across the monorepo.
- **Release profile is well-optimized**: `Cargo.toml` enables `lto = true`, `codegen-units = 1`, `strip = true`, and `opt-level = 3` — maximizing binary optimization for what does get compiled.
- **Platform-specific code uses `#[cfg(unix)]` / `#[cfg(windows)]` correctly**: 39+ platform-conditional blocks properly gate OS-specific code (signal handling, subprocess sandboxing, file paths), showing the team understands conditional compilation.
- **No duplicate dependency versions**: The `Cargo.lock` shows zero version conflicts — all 793 packages resolve to a single version each, avoiding diamond-dependency bloat.
- **Reqwest feature layering is correct**: `openfang-cli/Cargo.toml:29` adds `features = ["blocking"]` atop the workspace definition — Cargo's additive feature model means the CLI gets workspace features plus blocking, which is the correct pattern.
- **Target-specific dependencies used properly**: `libc` is correctly gated with `[target.'cfg(unix)'.dependencies]` in `openfang-kernel/Cargo.toml:37-38`, avoiding compilation on Windows.
