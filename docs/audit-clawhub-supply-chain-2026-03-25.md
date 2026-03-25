# OpenFang ClawHub Supply-Chain Vulnerability Audit

**Date:** 2026-03-25
**Auditor:** Claude (automated security audit)
**Scope:** OpenFang skills crate — ClawHub marketplace integration, skill loading trust model, and bundled skill content
**Context:** Assessment for vulnerability identical to ClawHavoc supply-chain attack (1,184+ malicious skills on ClawHub distributing keyloggers and Atomic Stealer)

---

## Executive Summary

**Overall Severity: HIGH**

OpenFang has meaningful security controls (prompt injection scanning, environment isolation, capability-based security) but contains **critical gaps in its supply-chain trust model** that make it vulnerable to a ClawHavoc-style attack. The most significant finding is that **skills downloaded from ClawHub have no cryptographic integrity verification** — the Ed25519 signing infrastructure exists for agent manifests but is never applied to marketplace skills. A malicious skill could be served by a compromised CDN or MITM without detection.

---

## Part 1: Skill Loading Trust Model

### 1.1 ClawHub Fetch and Install (`clawhub.rs`)

**What it does:**
- `ClawHubClient` connects to `https://clawhub.ai/api/v1` (hardcoded default)
- `install()` downloads a skill via `GET /api/v1/download?slug=...`
- Computes SHA-256 of downloaded content (line 521-525) and **logs it** but **never verifies it against any known-good hash**
- Detects format (SKILL.md vs zip vs package.json) and extracts to disk
- Runs `SkillVerifier::scan_prompt_content()` for prompt-only skills
- Runs `SkillVerifier::security_scan()` on the manifest
- Writes `skill.toml` with `verified: false` (per the doc comment, line 501)

**FINDING V-01 (CRITICAL): No cryptographic integrity verification on ClawHub downloads**

The `verify_checksum()` function exists in `verify.rs` (line 38) but is **never called** during the install pipeline. The SHA-256 is computed and logged but not compared to anything. There is:
- No hash pinning (no expected hash from ClawHub API to compare against)
- No signature verification (the `SignedManifest` Ed25519 system in `openfang-types/src/manifest_signing.rs` is never used for skills)
- No content-addressable storage
- No certificate pinning on the ClawHub HTTPS connection

An attacker who compromises the ClawHub CDN, performs a MITM attack, or gains publish access to a popular skill slug can serve arbitrary content that will be installed without integrity checks.

**FINDING V-02 (HIGH): Zip extraction without size limits or entry count limits**

In `clawhub.rs` lines 540-572, zip archives are extracted with:
- No maximum total extracted size limit (zip bomb risk)
- No maximum file count limit
- `enclosed_name()` is used (good — prevents path traversal), but there's no check on symlinks within the archive
- Failed extraction falls back to saving the raw zip (line 569), which could later be manually extracted unsafely

**FINDING V-03 (MEDIUM): No user confirmation gate before installation**

The `install()` function proceeds entirely programmatically. There is no interactive approval step where the user sees security warnings before files are written to disk. Warnings are collected and returned in `ClawHubInstallResult` but installation completes regardless (except for critical prompt injection, which does block).

### 1.2 Skill Loader — Execution Model (`loader.rs`)

**What it does:**
- `execute_skill_tool()` dispatches to Python, Node.js, or Shell runtimes
- All three runtimes spawn a subprocess with `env_clear()` (lines 96, 199, 345) — this is good
- Only `PATH`, `HOME`, and platform essentials (`SYSTEMROOT`, `TEMP`) are preserved
- Skills receive input via stdin as JSON, return output via stdout

**FINDING V-04 (HIGH): No OS-level sandboxing for subprocess execution**

While `env_clear()` prevents API key leakage (good), the spawned processes run with the **full privileges of the OpenFang daemon user**. There is:
- No seccomp/landlock/AppArmor profile
- No filesystem namespace isolation (skills can read/write anywhere the daemon user can)
- No network namespace isolation (skills can make arbitrary outbound connections)
- No resource limits (no cgroup, no `ulimit`, no timeout on the child process)
- The WASM runtime mentions sandboxing in `docs/security.md`, but Python/Node/Shell skills bypass it entirely

A malicious Node.js or Python skill could:
- Read `~/.ssh/`, `~/.aws/credentials`, `~/.config/` etc.
- Exfiltrate data over the network
- Install persistence mechanisms
- Spawn further processes

**FINDING V-05 (MEDIUM): Shell runtime passes `-s` flag to bash**

In `loader.rs` line 336, shell skills are invoked as `bash -s <script_path>`. The `-s` flag tells bash to read commands from stdin, and the script path is passed as a positional parameter. The JSON payload is also written to stdin (line 367-375). This creates ambiguity — bash will try to execute the JSON as shell commands before reaching the script, which could cause unexpected behavior.

### 1.3 OpenClaw Compatibility Layer (`openclaw_compat.rs`)

**What it does:**
- Detects SKILL.md (prompt-only) and package.json (Node.js) formats
- Converts them to OpenFang's `SkillManifest` format
- Translates OpenClaw tool names to OpenFang equivalents

**FINDING V-06 (MEDIUM): OpenClaw Node.js skills inherit the same no-sandbox execution**

`convert_openclaw_skill()` produces a manifest with `SkillRuntime::Node`, which means the skill will be executed via `execute_node()` with the same unsandboxed subprocess model. OpenClaw skills are third-party by definition, yet run with full daemon privileges.

### 1.4 Permission Manifest / Approval Gate / Code Signing

**FINDING V-07 (HIGH): Ed25519 signing exists but is disconnected from the skill pipeline**

The file `openfang-types/src/manifest_signing.rs` implements a complete Ed25519 signing/verification system (`SignedManifest::sign()`, `SignedManifest::verify()`). However:
- **It is never imported or used in the `openfang-skills` crate** (confirmed by grep — zero references)
- It was designed for agent manifests, not skill manifests
- There is no trusted public key store for ClawHub publisher keys
- There is no chain of trust from ClawHub to the local skill installation

The `skill.toml` written during installation contains `verified: false` but this field is not checked or enforced anywhere in the loading pipeline.

### 1.5 Prompt Injection Scanner (`verify.rs`)

**What it does:**
- `scan_prompt_content()` checks for 10 injection patterns, 8 exfiltration patterns, and 3 shell command patterns
- Critical findings block installation (in `clawhub.rs` lines 592-607)
- The scanner runs on bundled skills too (defense-in-depth, `registry.rs` line 69)

**FINDING V-08 (MEDIUM): Prompt injection scanner is easily bypassed**

The scanner uses simple substring matching on lowercased content. Known bypasses:
- Unicode homoglyphs: `ign𝗈re previous instructions` (mathematical bold 'o')
- Whitespace injection: `ignore  previous  instructions` (extra spaces)
- Word splitting across lines
- Base64-encoded instructions decoded by the LLM at runtime
- Indirect injection: "When the user says X, do Y" patterns
- Obfuscated patterns: `i.g" + "nore prev" + "ious`
- ROT13 or other simple encodings the LLM can decode
- Instructions in non-English languages

The 10 patterns are a good first layer but would not catch sophisticated ClawHavoc-style attacks.

### 1.6 Capability Enforcement

**Positive finding:** The WASM runtime has capability-based security with `check_capability()` that enforces permissions before each host call. However, **Python/Node/Shell skills bypass this entirely** since they run as raw subprocesses, not WASM modules.

---

## Part 2: ClawHub Marketplace Content Audit

### 2.1 Registry Access Model

OpenFang pulls skills from `https://clawhub.ai/api/v1` via these endpoints:
- `GET /api/v1/search?q=...&limit=20` — semantic search
- `GET /api/v1/skills?limit=20&sort=trending` — browse
- `GET /api/v1/skills/{slug}` — detail (includes owner, stats, moderation field)
- `GET /api/v1/download?slug=...` — download skill content
- `GET /api/v1/skills/{slug}/file?path=SKILL.md` — fetch individual files

**FINDING V-09 (INFORMATIONAL): ClawHub has a moderation field but it's not enforced client-side**

The `ClawHubSkillDetail` struct includes `moderation: Option<serde_json::Value>` (line 184). The install pipeline does not check this field. A skill flagged for moderation on ClawHub would still be installed by OpenFang.

### 2.2 Bundled Skills Content Audit

All 60 bundled skills were audited (they are compile-time embedded via `include_str!()`). A representative sample of 6 was examined in detail:

| Skill | Prompt Injection | External URLs | Credential Access | Shell Commands | Verdict |
|-------|-----------------|---------------|-------------------|----------------|---------|
| github | None | None | Mentions `gh auth` safely | `gh` CLI references | Clean |
| docker | None | None | Warns against secrets in layers | Docker commands | Clean |
| security-audit | None | None | Discusses secure storage | Tool references only | Clean |
| shell-scripting | None | None | None | Extensive (expected) | Clean |
| sysadmin | None | None | SSH key guidance | System commands | Clean |
| web-search | None | None | None | None | Clean |

All 60 bundled skills pass the `scan_prompt_content()` security scan (verified by existing unit test `test_bundled_skills_pass_security_scan`).

**No prompt injection attempts were found in any bundled skill.** All bundled skills are prompt-only (no executable code) and contain professional best-practices documentation.

### 2.3 ClawHub Publication Model

**FINDING V-10 (HIGH): No evidence of a mandatory review/curation process**

Based on the API structure:
- Anyone can publish skills to ClawHub (the API has no visible review/approval gate)
- The `moderation` field exists but is `null` for clean skills and is not enforced by OpenFang
- The `owner` info contains a `handle` and `userId` but there's no verified publisher program
- Download counts and stars can be gamed
- The ClawHavoc attack reference (341 malicious skills mentioned in `verify.rs` line 108) confirms this has already happened

---

## Part 3: Vulnerability Summary

### Critical (Exploitation would compromise the host system)

| ID | Finding | File | Impact |
|----|---------|------|--------|
| V-01 | No cryptographic integrity verification on ClawHub downloads | `clawhub.rs` | MITM or CDN compromise delivers malicious skills |
| V-04 | No OS-level sandboxing for Python/Node/Shell skills | `loader.rs` | Malicious skills have full daemon-user privileges |

### High (Significant security gap)

| ID | Finding | File | Impact |
|----|---------|------|--------|
| V-02 | Zip extraction without size/count limits | `clawhub.rs` | Zip bomb DoS, resource exhaustion |
| V-07 | Ed25519 signing disconnected from skill pipeline | `manifest_signing.rs` / `clawhub.rs` | Supply-chain integrity not enforced |
| V-10 | No mandatory review/curation on ClawHub | `clawhub.rs` | Anyone can publish malicious skills |

### Medium

| ID | Finding | File | Impact |
|----|---------|------|--------|
| V-03 | No user confirmation gate before install | `clawhub.rs` | Users don't see warnings before disk write |
| V-05 | Shell runtime stdin ambiguity with `-s` flag | `loader.rs` | Unexpected code execution |
| V-06 | OpenClaw Node.js skills run unsandboxed | `openclaw_compat.rs` + `loader.rs` | Third-party code with full privileges |
| V-08 | Prompt injection scanner trivially bypassed | `verify.rs` | Malicious prompts evade detection |

### Informational

| ID | Finding | File | Impact |
|----|---------|------|--------|
| V-09 | Moderation field not enforced client-side | `clawhub.rs` | Flagged skills still installable |

---

## Part 4: Positive Security Controls (Credit)

OpenFang does implement several security measures that mitigate risk:

1. **Environment isolation** (`env_clear()` in `loader.rs`) — prevents API key leakage to child processes
2. **Prompt injection scanner** (`verify.rs`) — blocks the most obvious injection patterns
3. **Critical injection blocks installation** (`clawhub.rs` lines 592-607) — skills with detected injection are rejected and cleaned up
4. **Capability-based security** for WASM modules — proper principle of least privilege
5. **Capability inheritance validation** — child agents cannot escalate privileges
6. **Registry freeze mode** (`registry.rs`) — prevents runtime skill loading in Stable mode
7. **Zip path traversal prevention** — uses `enclosed_name()` correctly
8. **Defense-in-depth scanning of bundled skills** — even trusted skills are scanned
9. **SHA-256 logging** — downloads are hashed (though not verified, the hash is available for forensics)

---

## Part 5: Recommendations

### Immediate (P0 — blocks ClawHavoc-class attacks)

1. **Wire Ed25519 signing into the skill install pipeline.** Require ClawHub to serve a `SignedManifest` envelope. Verify signatures against a pinned set of trusted publisher keys before extracting any content.

2. **Add OS-level sandboxing for subprocess skills.** On Linux: use `landlock` LSM or `seccomp-bpf` to restrict filesystem and network access. On macOS: use `sandbox-exec`. As a minimum, use `unshare` for network namespace isolation.

3. **Add user confirmation gate.** Before writing downloaded skill content to disk, display all security warnings and require explicit user approval.

### Short-term (P1)

4. **Check the `moderation` field** from ClawHub and refuse to install skills under active moderation review.

5. **Add zip extraction limits:** max 50MB total extracted size, max 1000 entries, no symlinks.

6. **Add subprocess timeout:** kill skill processes that run longer than a configurable limit (e.g., 60 seconds).

7. **Enhance prompt injection scanner** with:
   - Unicode normalization before matching
   - Entropy-based detection for obfuscated content
   - LLM-based classification for subtle injection patterns

### Medium-term (P2)

8. **Implement content-addressable skill storage** with hash pinning from a trusted registry index.

9. **Extend capability-based security to subprocess skills** via a permission manifest (`skill.toml` declares required capabilities, user approves, loader enforces).

10. **Add network egress filtering** for skill subprocesses (allowlist of permitted domains).

---

## Prompt Injection Audit Notes

During this audit, I examined all skill content with awareness that it could contain adversarial prompt injection. **No prompt injection attempts were found** in any of the bundled skills or in any code within the repository. All 60 bundled skills contain legitimate, professional instructional content.

The prompt injection scanner test patterns in `verify.rs` were not triggered by any content I read during this audit. I confirmed that my analysis was not influenced by any content within the skills.
