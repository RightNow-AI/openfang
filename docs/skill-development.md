# Skill Development

Skills are pluggable tool bundles that extend agent capabilities in OpenFang. A skill packages one or more tools with their implementation, letting agents do things that built-in tools do not cover. This guide covers skill creation, the manifest format, Python and WASM runtimes, publishing to FangHub, and CLI management.

## Table of Contents

- [Overview](#overview)
- [Skill Format](#skill-format)
- [Python Skills](#python-skills)
- [WASM Skills](#wasm-skills)
- [Skill Requirements](#skill-requirements)
- [Installing Skills](#installing-skills)
- [Publishing to FangHub](#publishing-to-fanghub)
- [CLI Commands](#cli-commands)
- [OpenClaw Compatibility](#openclaw-compatibility)
- [Best Practices](#best-practices)

---

## Overview

A skill consists of:

1. A **manifest** (`skill.toml` or `SKILL.md`) that declares metadata, runtime type, provided tools, and requirements.
2. An **entry point** (Python script, WASM module, Node.js module, or prompt-only Markdown) that implements the tool logic.

Skills are installed to `~/.openfang/skills/` and made available to agents through the skill registry. OpenFang ships with **60 bundled skills** that are compiled into the binary and available immediately.

### Supported Runtimes

| Runtime | Language | Sandboxed | Notes |
|---------|----------|-----------|-------|
| `python` | Python 3.8+ | No (subprocess with `env_clear()`) | Easiest to write. Uses stdin/stdout JSON protocol. |
| `wasm` | Rust, C, Go, etc. | Yes (Wasmtime dual metering) | Fully sandboxed. Best for security-sensitive tools. |
| `node` | JavaScript/TypeScript | No (subprocess) | OpenClaw compatibility. |
| `prompt_only` | Markdown | N/A | Expert knowledge injected into system prompt. No code execution. |
| `builtin` | Rust | N/A | Compiled into the binary. For core tools only. |

### 60 Bundled Skills

OpenFang includes 60 expert knowledge skills compiled into the binary (no installation needed):

| Category | Skills |
|----------|--------|
| DevOps & Infra | `ci-cd`, `ansible`, `prometheus`, `nginx`, `kubernetes`, `terraform`, `helm`, `docker`, `sysadmin`, `shell-scripting`, `linux-networking` |
| Cloud | `aws`, `gcp`, `azure` |
| Languages | `rust-expert`, `python-expert`, `typescript-expert`, `golang-expert` |
| Frontend | `react-expert`, `nextjs-expert`, `css-expert` |
| Databases | `postgres-expert`, `redis-expert`, `sqlite-expert`, `mongodb`, `elasticsearch`, `sql-analyst` |
| APIs & Web | `graphql-expert`, `openapi-expert`, `api-tester`, `oauth-expert` |
| AI/ML | `ml-engineer`, `llm-finetuning`, `vector-db`, `prompt-engineer` |
| Security | `security-audit`, `crypto-expert`, `compliance` |
| Dev Tools | `github`, `git-expert`, `jira`, `linear-tools`, `sentry`, `code-reviewer`, `regex-expert` |
| Writing | `technical-writer`, `writing-coach`, `email-writer`, `presentation` |
| Data | `data-analyst`, `data-pipeline` |
| Collaboration | `slack-tools`, `notion`, `confluence`, `figma-expert` |
| Career | `interview-prep`, `project-manager` |
| Advanced | `wasm-expert`, `pdf-reader`, `web-search` |

These are `prompt_only` skills using the SKILL.md format -- expert knowledge that gets injected into the agent's system prompt.

### SKILL.md Format

The SKILL.md format (also used by OpenClaw) uses YAML frontmatter and a Markdown body:

```markdown
---
name: rust-expert
description: Expert Rust programming knowledge
---

# Rust Expert

## Key Principles
- Ownership and borrowing rules...
- Lifetime annotations...

## Common Patterns
...
```

SKILL.md files are automatically parsed and converted to `prompt_only` skills. All SKILL.md files pass through an automated **prompt injection scanner** that detects override attempts, data exfiltration patterns, and shell references before inclusion.

---

## Skill Format

### Directory Structure

```
my-skill/
  skill.toml          # Manifest (required)
  src/
    main.py           # Entry point (for Python skills)
  README.md           # Optional documentation
```

### Manifest (skill.toml)

```toml
[skill]
name = "web-summarizer"
version = "0.1.0"
description = "Summarizes any web page into bullet points"
author = "openfang-community"
license = "MIT"
tags = ["web", "summarizer", "research"]

[runtime]
type = "python"
entry = "src/main.py"

[[tools.provided]]
name = "summarize_url"
description = "Fetch a URL and return a concise bullet-point summary"
input_schema = { type = "object", properties = { url = { type = "string", description = "The URL to summarize" } }, required = ["url"] }

[[tools.provided]]
name = "extract_links"
description = "Extract all links from a web page"
input_schema = { type = "object", properties = { url = { type = "string" } }, required = ["url"] }

[requirements]
tools = ["web_fetch"]
capabilities = ["NetConnect(*)"]
```

### Manifest Sections

#### [skill] -- Metadata

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Unique skill name (used as install directory name) |
| `version` | string | No | Semantic version (default: `"0.1.0"`) |
| `description` | string | No | Human-readable description |
| `author` | string | No | Author name or organization |
| `license` | string | No | License identifier (e.g., `"MIT"`, `"Apache-2.0"`) |
| `tags` | array | No | Tags for discovery on FangHub |

#### [runtime] -- Execution Configuration

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | Yes | `"python"`, `"wasm"`, `"node"`, or `"builtin"` |
| `entry` | string | Yes | Relative path to the entry point file |

#### [[tools.provided]] -- Tool Definitions

Each `[[tools.provided]]` entry defines one tool that the skill provides:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Tool name (must be unique across all tools) |
| `description` | string | Yes | Description shown to the LLM |
| `input_schema` | object | Yes | JSON Schema defining the tool's input parameters |

#### [requirements] -- Host Requirements

| Field | Type | Description |
|-------|------|-------------|
| `tools` | array | Built-in tools this skill needs the host to provide |
| `capabilities` | array | Capability strings the agent must have |

---

## Python Skills

Python skills are the simplest to write. They run as subprocesses and communicate via JSON over stdin/stdout.

### Protocol

1. OpenFang sends a JSON payload to the script's stdin:

```json
{
  "tool": "summarize_url",
  "input": {
    "url": "https://example.com"
  },
  "agent_id": "uuid-...",
  "agent_name": "researcher"
}
```

2. The script processes the input and writes a JSON result to stdout:

```json
{
  "result": "- Point one\n- Point two\n- Point three"
}
```

If an error occurs, return an error object:

```json
{
  "error": "Failed to fetch URL: connection refused"
}
```

### Example: Web Summarizer

`src/main.py`:

```python
#!/usr/bin/env python3
"""OpenFang skill: web-summarizer"""
import json
import sys
import urllib.request


def summarize_url(url: str) -> str:
    """Fetch a URL and return a basic summary."""
    req = urllib.request.Request(url, headers={"User-Agent": "OpenFang-Skill/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        content = resp.read().decode("utf-8", errors="replace")

    # Simple extraction: first 500 chars as summary
    text = content[:500].strip()
    return f"Summary of {url}:\n{text}..."


def extract_links(url: str) -> str:
    """Extract all links from a web page."""
    import re

    req = urllib.request.Request(url, headers={"User-Agent": "OpenFang-Skill/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        content = resp.read().decode("utf-8", errors="replace")

    links = re.findall(r'href="(https?://[^"]+)"', content)
    unique_links = list(dict.fromkeys(links))
    return "\n".join(unique_links[:50])


def main():
    payload = json.loads(sys.stdin.read())
    tool_name = payload["tool"]
    input_data = payload["input"]

    try:
        if tool_name == "summarize_url":
            result = summarize_url(input_data["url"])
        elif tool_name == "extract_links":
            result = extract_links(input_data["url"])
        else:
            print(json.dumps({"error": f"Unknown tool: {tool_name}"}))
            return

        print(json.dumps({"result": result}))
    except Exception as e:
        print(json.dumps({"error": str(e)}))


if __name__ == "__main__":
    main()
```

### Using the OpenFang Python SDK

For more advanced skills, use the Python SDK (`sdk/python/openfang_sdk.py`):

```python
#!/usr/bin/env python3
from openfang_sdk import SkillHandler

handler = SkillHandler()

@handler.tool("summarize_url")
def summarize_url(url: str) -> str:
    # Your implementation here
    return "Summary..."

@handler.tool("extract_links")
def extract_links(url: str) -> str:
    # Your implementation here
    return "link1\nlink2"

if __name__ == "__main__":
    handler.run()
```

---

## WASM Skills

WASM skills run inside a sandboxed Wasmtime environment. They are ideal for security-sensitive operations because the sandbox enforces resource limits and capability restrictions.

### Building a WASM Skill

1. Write your skill in Rust (or any language that compiles to WASM):

```rust
// src/lib.rs
use std::io::{self, Read};

#[no_mangle]
pub extern "C" fn _start() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();

    let payload: serde_json::Value = serde_json::from_str(&input).unwrap();
    let tool = payload["tool"].as_str().unwrap_or("");
    let input_data = &payload["input"];

    let result = match tool {
        "my_tool" => {
            let param = input_data["param"].as_str().unwrap_or("");
            format!("Processed: {param}")
        }
        _ => format!("Unknown tool: {tool}"),
    };

    println!("{}", serde_json::json!({"result": result}));
}
```

2. Compile to WASM:

```bash
cargo build --target wasm32-wasi --release
```

3. Reference the `.wasm` file in your manifest:

```toml
[runtime]
type = "wasm"
entry = "target/wasm32-wasi/release/my_skill.wasm"
```

### Sandbox Limits

The WASM sandbox enforces:

- **Fuel limit**: Maximum computation steps (prevents infinite loops).
- **Memory limit**: Maximum memory allocation.
- **Capabilities**: Only the capabilities granted to the agent apply.

These are derived from the agent's `[resources]` section in its manifest.

---

## Skill Requirements

Skills can declare requirements in the `[requirements]` section:

### Tool Requirements

If your skill needs to call built-in tools (e.g., `web_fetch` to download a page before processing it):

```toml
[requirements]
tools = ["web_fetch", "file_read"]
```

The skill registry validates that the agent has these tools available before loading the skill.

### Capability Requirements

If your skill needs specific capabilities:

```toml
[requirements]
capabilities = ["NetConnect(*)", "ShellExec(python3)"]
```

---

## Installing Skills

### From a Local Directory

```bash
openfang skill install /path/to/my-skill
```

This reads the `skill.toml`, validates the manifest, and copies the skill to `~/.openfang/skills/my-skill/`.

### From FangHub

```bash
openfang skill install web-summarizer
```

This downloads the skill from the FangHub marketplace registry.

### From a Git Repository

```bash
openfang skill install https://github.com/user/openfang-skill-example.git
```

### Listing Installed Skills

```bash
openfang skill list
```

Output:

```
3 skill(s) installed:

NAME                 VERSION    TOOLS    DESCRIPTION
----------------------------------------------------------------------
web-summarizer       0.1.0      2        Summarizes any web page into bullet points
data-analyzer        0.2.1      3        Statistical analysis tools
code-formatter       1.0.0      1        Format code in 20+ languages
```

### Removing Skills

```bash
openfang skill remove web-summarizer
```

---

## Publishing to FangHub

FangHub is the community skill marketplace for OpenFang.

### Preparing Your Skill

1. Ensure your `skill.toml` has complete metadata:
   - `name`, `version`, `description`, `author`, `license`, `tags`
2. Include a `README.md` with usage instructions.
3. Test your skill locally:

```bash
openfang skill install /path/to/my-skill
# Spawn an agent with the skill's tools and test them
```

### Searching FangHub

```bash
openfang skill search "web scraping"
```

Output:

```
Skills matching "web scraping":

  web-summarizer (42 stars)
    Summarizes any web page into bullet points
    https://fanghub.dev/skills/web-summarizer

  page-scraper (28 stars)
    Extract structured data from web pages
    https://fanghub.dev/skills/page-scraper
```

### Publishing

Publishing to FangHub will be available via:

```bash
openfang skill publish
```

This validates the manifest, packages the skill, and uploads it to the FangHub registry.

---

## CLI Commands

### Full Skill Command Reference

```bash
# Install a skill (local directory, FangHub name, or git URL)
openfang skill install <source>

# List all installed skills
openfang skill list

# Remove an installed skill
openfang skill remove <name>

# Search FangHub for skills
openfang skill search <query>

# Create a new skill scaffold (interactive)
openfang skill create
```

### Creating a Skill Scaffold

```bash
openfang skill create
```

This interactive command prompts for:
- Skill name
- Description
- Runtime type (python/node/wasm)

It generates:

```
~/.openfang/skills/my-skill/
  skill.toml        # Pre-filled manifest
  src/
    main.py         # Starter entry point (for Python)
```

The generated entry point includes a working template that reads JSON from stdin and writes JSON to stdout.

### Using Skills in Agent Manifests

Reference skills in the agent manifest's `skills` field:

```toml
name = "my-assistant"
version = "0.1.0"
description = "An assistant with extra skills"
author = "openfang"
module = "builtin:chat"
skills = ["web-summarizer", "data-analyzer"]

[model]
provider = "groq"
model = "llama-3.3-70b-versatile"

[capabilities]
tools = ["file_read", "web_fetch", "summarize_url"]
memory_read = ["*"]
memory_write = ["self.*"]
```

The kernel loads skill tools and prompts at agent spawn time, merging them with the agent's base capabilities.

---

## OpenClaw Compatibility

OpenFang can install and run OpenClaw-format skills. The skill installer auto-detects OpenClaw skills (by looking for `package.json` + `index.ts`/`index.js`) and converts them.

### Automatic Conversion

```bash
openfang skill install /path/to/openclaw-skill
```

If the directory contains an OpenClaw-style skill (Node.js package), OpenFang:

1. Detects the OpenClaw format.
2. Generates a `skill.toml` manifest from `package.json`.
3. Maps tool names to OpenFang conventions.
4. Copies the skill to the OpenFang skills directory.

### Manual Conversion

If automatic conversion does not work, create a `skill.toml` manually:

```toml
[skill]
name = "my-openclaw-skill"
version = "1.0.0"
description = "Converted from OpenClaw"

[runtime]
type = "node"
entry = "index.js"

[[tools.provided]]
name = "my_tool"
description = "Tool description"
input_schema = { type = "object", properties = { input = { type = "string" } }, required = ["input"] }
```

Place this alongside the existing `index.js`/`index.ts` and install:

```bash
openfang skill install /path/to/skill-directory
```

Skills imported via `openfang migrate --from openclaw` are also scanned and reported in the migration report, with instructions for manual reinstallation.

---

## Best Practices

1. **Keep skills focused** -- one skill should do one thing well.
2. **Declare minimal requirements** -- only request the tools and capabilities your skill actually needs.
3. **Use descriptive tool names** -- the LLM reads the tool name and description to decide when to use it.
4. **Provide clear input schemas** -- include descriptions for every parameter so the LLM knows what to pass.
5. **Handle errors gracefully** -- always return a JSON error object rather than crashing.
6. **Version carefully** -- use semantic versioning; breaking changes require a major version bump.
7. **Test with multiple agents** -- verify your skill works with different agent templates and providers.
8. **Include a README** -- document setup steps, dependencies, and example usage.
