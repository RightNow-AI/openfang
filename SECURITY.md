# Security Policy

OpenFang is an agent operating system that executes AI agents, manages external integrations, handles financial data, and runs user-defined tools and skills. Security is a first-class concern at every layer.

This document is the external-facing security policy. For implementation details see:
- [`docs/security-architecture.md`](docs/security-architecture.md)
- [`docs/auth-matrix.md`](docs/auth-matrix.md)
- [`docs/threat-model.md`](docs/threat-model.md)
- [`docs/incident-response.md`](docs/incident-response.md)
- [`prompts/security-guardrail.md`](prompts/security-guardrail.md)

---

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest release on `main` | ✅ Active |
| Prior minor releases (< 30 days) | ⚠️ Best-effort |
| Older releases | ❌ No patches |

Only the latest release receives security fixes. Upgrade before reporting.

---

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report privately via one of:

1. **GitHub Security Advisories** — [github.com/RightNow-AI/openfang/security/advisories/new](https://github.com/RightNow-AI/openfang/security/advisories/new)
2. **Email** — security@rightnow.ai (PGP key available on request)

Include in your report:
- Affected component, version, and endpoint
- Step-by-step reproduction and proof-of-concept where possible
- Impact assessment and proposed severity (CVSS if available)
- Whether you need coordinated disclosure time before public release

We will acknowledge within **2 business days** and provide a remediation timeline within **7 business days**. Critical vulnerabilities targeting production deployments will be patched within **14 days**.

---

## Disclosure Timeline

| Stage | Target |
|-------|--------|
| Acknowledgement | ≤ 2 business days |
| Triage and severity assignment | ≤ 5 business days |
| Remediation timeline communicated | ≤ 7 business days |
| Patch for Critical/High | ≤ 14 days |
| Patch for Medium | ≤ 45 days |
| Patch for Low/Informational | Next scheduled release |
| Public disclosure | After patch ships or 90 days, whichever comes first |

We follow coordinated disclosure. We will credit researchers who report responsibly unless they prefer anonymity.

---

## Scope

The following are in scope for security reports:

- **Rust daemon** (`crates/openfang-api`, `crates/openfang-kernel`, `crates/openfang-runtime`, `crates/openfang-skills`, `crates/openfang-hands`, `crates/openfang-orchestrator`, `crates/openfang-channels`)
- **API surface** — all endpoints under `/api/`
- **Agent execution and skill system** — agent spawning, tool dispatch, capability enforcement
- **Approval and authorization flows** — finance, comms, integrations, external agent sends
- **Next.js frontend** (`sdk/javascript/examples/nextjs-app-router`) — auth, BFF routes, secrets handling
- **CLI** (`crates/openfang-cli`)
- **Configuration and secrets handling** — `config.toml`, environment variables
- **OFP network and A2A protocol** — peer trust, task routing
- **Supply chain** — Cargo.lock, package-lock.json, GitHub Actions workflows
- **CI/CD pipeline** — injection into build or release pipeline

### Out of Scope

The following will not be accepted as valid security reports:

- Self-XSS that requires an authenticated user to paste code into the browser console
- Rate-limit bypass claims submitted without a working reproduction against a real deployment
- Issues already acknowledged in the public issue tracker or a prior advisory
- Theoretical vulnerabilities with no demonstrated or credible exploit path
- Missing security headers on localhost development server
- Social engineering attacks on project maintainers
- Physical access attacks
- DoS via resource exhaustion on an authenticated local instance without privilege escalation
- Findings generated purely by automated scanners without manual validation

---

## Security Architecture

Full detail: [`docs/security-architecture.md`](docs/security-architecture.md)

### Authentication and Authorization

- All non-public API endpoints require authentication. See [`docs/auth-matrix.md`](docs/auth-matrix.md) for the per-endpoint matrix.
- The default deployment model is local loopback (`127.0.0.1:50051`). Loopback trust is explicit and documented; it does not substitute for auth when the API is exposed to a network.
- Multi-user and network deployments must enable API key or token-based auth in `config.toml`. Exposing the API on `0.0.0.0` without auth is a misconfiguration, not a design choice.
- Agent capability enforcement is server-side. UI-level checks are informational only.
- Child agents inherit at most the capabilities of their parent. Agents cannot self-escalate.
- Approval state for high-risk actions (finance, comms, external sends, tool use) is validated server-side on every execution. Approval cannot be bypassed by replaying or modifying client requests.

### CSRF Policy

- All state-mutating API endpoints that may be called from browser contexts require either:
  - An `Authorization` header (not settable by cross-origin form posts), or
  - A validated CSRF token for session-cookie–authenticated routes
- `SameSite=Strict` cookies are used where cookies are issued
- Cross-origin requests are controlled by the configured CORS allowlist

### Input Validation

- All request bodies, query parameters, and path segments are validated against strict schemas at the API boundary.
- Unknown fields are rejected by default. Explicit allow-listing is required.
- Mass assignment is prevented by explicit field mapping on every write operation.
- Redirect and callback URLs are validated against a configured allowlist. Open redirects are treated as High severity.
- Filenames, paths, and uploaded content metadata are sanitized before use.

### Uploads

- Uploaded files are stored outside the public web root with server-controlled access.
- MIME type is validated against both the `Content-Type` header and the actual file signature (magic bytes).
- File size limits are enforced at the ingestion layer, not only in the client.
- File extension alone is never trusted for type decisions.
- Download access is served through controlled endpoints with ownership checks, not by direct path exposure.
- Uploads to be executed (e.g., skill packages) go through a quarantine validation step before activation.

### Network Egress and SSRF

- Outbound fetch from agent tools, skill runners, and integration adapters is filtered through the SSRF protection layer.
- The following destinations are blocked by default unless explicitly allowlisted in `config.toml`:
  - Private IP ranges (RFC 1918: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
  - Loopback (127.0.0.0/8, ::1)
  - Link-local (169.254.0.0/16, fe80::/10)
  - Cloud metadata services (169.254.169.254, [fd00:ec2::254], metadata.google.internal, etc.)
  - Internal control plane addresses
- DNS is resolved and validated immediately before connection for sensitive destinations.
- Allowlist mode is available for deployments that need strict egress control.
- Rate limiting applies to all external fetch endpoints.

### Secrets Handling

- Secrets (API keys, tokens, credentials) are never hardcoded in source.
- Secrets are never serialized into client-side JavaScript bundles.
- Secrets are sourced from environment variables or a configured secrets reference only.
- Secrets are redacted from all log output, error messages, audit records, and telemetry.
- Any accidental commit of a secret triggers immediate rotation; the commit is not merely reverted.
- Key rotation procedure: see [`docs/incident-response.md`](docs/incident-response.md).

### Approval and High-Risk Actions

High-risk action classes that require explicit approval state validation before execution:

| Class | Approval Required |
|-------|-------------------|
| Finance — execute transaction or transfer | ✅ Required |
| Finance — update budget ceiling | ✅ Required |
| Investments — place order or modify position | ✅ Required |
| Comms — send external message (email, SMS, webhook) | ✅ Required (configurable) |
| Integrations — write to external service | ✅ Required (configurable) |
| Agent tool use — external API call | ✅ Required (configurable per agent) |
| Agent spawn — create child agent | ✅ Required for high-capability agents |
| Workflow trigger — external action sequence | ✅ Required (configurable) |
| Skills — install new skill package | ✅ Required |
| Config — credential or key change | ✅ Required |

Approval state is never read from the client request. It is read from the server-side approval store only.

Proposal (generating a plan, drAfting a message) is always permitted without approval. **Execution is not.**

### Audit Logging

The following action classes are always audit-logged:

- Agent spawn, kill, and configuration change
- Tool use and external API calls
- Approval decisions (grant and reject)
- Finance and investment action proposals and executions
- Comms sends
- Workflow and scheduler trigger
- Skill install, update, and delete
- Export and data download
- Auth events (login, logout, token issue, key rotation)
- Role and permission changes
- Any error resulting in a 5xx response for high-risk endpoints

Audit logs must not contain raw secret values or tokens. They must be append-only in standard deployments and tamper-evident in hardened deployments.

### Dependencies and Supply Chain

- Rust dependencies are pinned via `Cargo.lock`. `cargo audit` runs in CI.
- JavaScript dependencies are pinned via `package-lock.json`. `npm audit` runs in CI.
- Secret scanning runs on every push and pull request via the CI security workflow.
- SBOM is generated on every release.
- Security-critical dependencies are reviewed for active maintenance before adoption.
- Abandoned or unmaintained packages used in security-sensitive paths are replaced.

### Rate Limiting

| Endpoint Class | Limit |
|----------------|-------|
| Auth endpoints | 10 req/min per IP |
| Agent message (LLM-triggering) | 60 req/min per authenticated user |
| External send (comms, integrations) | 30 req/min per authenticated user |
| Finance/investment write actions | 10 req/min per authenticated user |
| Skill install | 5 req/min per authenticated user |
| General read endpoints | 300 req/min per authenticated user |
| Health endpoint (public) | 120 req/min per IP |

Rate limits are enforced server-side. Client-side throttling is not a substitute.

### Logging Redaction Rules

The following are always redacted from logs before writing, regardless of log level:

- API keys, bearer tokens, session tokens
- Passwords and passphrases
- Private keys and secrets
- Credit card numbers, bank account numbers
- Personally identifiable information in tool call arguments (configurable)
- LLM prompt content containing user credentials or payment data

Redaction is applied at the log sink layer, not by asking callers to self-censor.

---

## Model and Tool Allowlists

For high-risk agent routes (finance, comms, integrations, browser automation via Hands), the permitted model list and tool list are enforced by the kernel, not the agent. Agents running in constrained modes cannot call models or tools outside their approved set.

Default high-risk tool blocklist applies to untrusted skill packages:
- Direct filesystem write outside the agent sandbox
- Network fetch without going through the SSRF-protected adapter
- Process execution
- Access to secrets or config files

---

## Incident Response

Full procedure: [`docs/incident-response.md`](docs/incident-response.md)

**Immediate steps when a credential or secret is confirmed exposed:**

1. Rotate the affected credential immediately — before analysis, before postmortem
2. Invalidate all active sessions that used the credential
3. Review audit logs for use of the credential since its creation date
4. Assess blast radius and data accessed
5. Notify affected users if data exposure is confirmed
6. File a private security advisory
7. Document timeline and close the loop

**Key rotation contacts and procedures** are described in `docs/incident-response.md`.

---

## OpenFang-Specific Threat Classes

The following threat classes are explicitly in scope and tracked in [`docs/threat-model.md`](docs/threat-model.md):

| Threat | Surface |
|--------|---------|
| Malicious skill package install | Skills system |
| Prompt injection via skill metadata or registry content | Skills, agents |
| Agent-to-agent privilege escalation | Orchestrator, A2A |
| Approval bypass on external actions | Approval flows |
| Unsafe loopback trust (API exposed without auth) | API server |
| Model/tool route abuse by rogue agents | Kernel, runtime |
| File exfiltration through tool calls | Hands, tools |
| Scheduler/workflow abuse for persistence | Scheduler, workflows |
| Comms abuse for spam or phishing | Channels, comms |
| Finance/investment execution without approval | Finance, investment routes |
| Browser automation abuse via Hands | Hands |
| Long-running job resource exhaustion | Runtime, scheduler |
| Desktop/mobile packaging secrets leakage | Desktop, mobile |
| SSRF through integration adapters | Channels, hands, skills |
| Unsafe deserialization of agent messages | Wire, OFP protocol |

---

*Last updated: 2026-03-19*