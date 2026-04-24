# Cyber-Intelligence Vault Setup

The cyber-agent (Phase 5.3 of the v0.6.1 hardening) classifies quarantined external content as `safe` / `questionable` / `malicious`. Its verdicts gate what reaches the native `MemorySubstrate`, the Obsidian vault, and Mempalace, so the quality of its decisions is bounded by the cyber-intelligence reference material it has at hand.

This document specifies the **expected layout** of the cyber-intelligence vault. OpenFang **consumes** this layout; the user is responsible for **populating** it (PDF→Markdown conversion + curation are out of scope for the OpenFang codebase).

## Configured path

In `~/.openfang/config.toml`:

```toml
[security.cyber_intel_vault]
path = "~/Documents/ObsidianVault/CyberIntel"
```

The kernel's boot-warm path verifies this directory exists when the cyber-agent is enabled. If absent or unreadable, boot fails with a pointer to this document.

## Expected layout

```
CyberIntel/
├── papers/                       # Curated security papers (PDF → Markdown)
│   ├── lateral-movement-2024.md
│   ├── ssrf-techniques-2025.md
│   └── prompt-injection-survey.md
├── frameworks/                   # MITRE ATT&CK / NIST / CIS excerpts
│   ├── mitre-attack-t1059.md     # Command and Scripting Interpreter
│   ├── mitre-attack-t1078.md     # Valid Accounts
│   ├── nist-csf-pr-ac.md         # Identity Mgmt & Access Control
│   └── cis-control-3.md          # Data Protection
├── indicators.yaml               # Hand-maintained IoC list (see schema below)
└── README.md                     # Operator notes (free-form)
```

### `indicators.yaml` schema

```yaml
# Each top-level key is an IoC family name. Entries are matched verbatim
# against the content summary the cyber-agent receives — no regex, no
# partial matches, but case-insensitive.

cobalt_strike:
  description: "Beacon C2 strings or default named pipes."
  indicators:
    - "ReflectiveLoader"
    - "msagent_"
    - "wkssvc"
  severity: high

emotet:
  description: "Emotet downloader patterns."
  indicators:
    - "ws.cloud.bizreqs"
    - "/MsoftUpd"
  severity: high
```

OpenFang does not ship a parser for `indicators.yaml` — it's surfaced verbatim into the cyber-agent's system prompt as part of the cyber-intel excerpts string. The agent reasons about it natively.

## How the cyber-agent uses the vault

At classification time, the kernel's classifier orchestrator:

1. Reads up to N most-recent files under `frameworks/` (default 8).
2. Reads `indicators.yaml` if present.
3. Concatenates everything into a `cyber_intel_excerpts` string.
4. Feeds the excerpts into [`triage::classifier::build_system_prompt`](../../crates/openfang-runtime/src/triage/classifier.rs).

The `papers/` dir is not loaded at runtime — those are reference material for the operator and for any future RAG / embedding pipeline. OpenFang does not currently embed or search them.

## Recommended starter content

If you're populating the vault from scratch:

- **MITRE ATT&CK Enterprise** — at minimum the techniques OpenFang's hand-set already touches: T1059 (Command and Scripting Interpreter), T1071 (Application Layer Protocol), T1090 (Proxy), T1567 (Exfiltration Over Web Service), T1190 (Exploit Public-Facing Application), T1620 (Reflective Code Loading).
- **OWASP LLM Top 10** — particularly LLM01 (Prompt Injection), LLM02 (Insecure Output Handling), LLM06 (Sensitive Information Disclosure), LLM08 (Excessive Agency), LLM10 (Model Theft).
- **MITRE ATLAS** — the AI/ML adversarial-techniques companion to ATT&CK.
- **A list of recently-disclosed package-supply-chain incidents** that map to the channels OpenFang's hands operate over (npm, PyPI, browser extensions).

## What NOT to put in the vault

- Live exploit code or weaponised payloads. The cyber-agent does not need them; their presence increases the blast radius if the vault is ever exfiltrated.
- Anything that contains real credentials, customer data, or secrets. The vault is read into the agent's prompt verbatim.
- Anything classified at a level the operator can't share with a frontier-model API.

## Verifying after setup

```bash
# After populating the vault and configuring the path:
$ openfang start
# Expected: kernel boot-warm logs `cyber_intel_vault: ok (N frameworks, M papers)`
# Failure: `cyber_intel_vault: configured path missing — see docs/security/cyber-intel-vault-setup.md`
```

The boot-warm wiring lands in **Phase 6** (P6.1 — boot-warm health gating). Until then, the cyber-agent runs with whatever excerpts the orchestrator manually assembles, or with `(no cyber-intelligence excerpts loaded)` in the prompt — both are functional but degrade decision quality.
