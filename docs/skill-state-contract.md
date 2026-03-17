# Skill State and Usage Contract

> **Status:** Active — Phase 2  
> **Last updated:** 2026-03-17  

---

## 1. Core terms

**Skill**  
A package or built-in unit that exposes zero or more tools and may also include prompt context or runtime code.

**Tool**  
A callable capability exposed by a skill.

**Installed**  
The skill exists in the system and its metadata is discoverable.

**Enabled**  
The skill is allowed to be invoked by the runtime for future work.

**Attached**  
Reserved for a future explicit agent-to-skill binding model. Not used as the current source of truth.

**Used by / Referenced by**  
An agent is counted as using a skill when it references one or more tools exposed by that skill.

> **This line prevents later arguments.** An agent is counted once per skill, regardless of how many of that skill's tools it references. Configuration-derived, not runtime-verified.

---

## 2. Current truth model

For Phase 2, usage is **derived**, not explicit.

**Source of truth:** agent config `capabilities.tools`

**Derived mapping:**
```
tool name  →  skill name
agent      →  referenced tools
skill      →  agents referencing one or more tools from that skill
```

**Critical rule:**  
`used_by` does **not** mean the agent explicitly attaches the skill.  
It means the agent references at least one tool provided by the skill.

This count is configuration-derived and does not imply an explicit `skills = [...]` binding.

---

## 3. State rules

| State | Meaning |
|---|---|
| `installed: true` | Skill is present in registry or filesystem and can be loaded for metadata |
| `enabled: true` | Skill is eligible for runtime invocation |
| `used_by_count: N` | N distinct agents reference one or more tools from this skill |

### Disabling a skill

- Blocks future runtime invocations
- Does **not** rewrite agent configs
- Does **not** detach tool references
- Does **not** stop in-flight work unless the runtime already supports safe interruption

---

## 4. Invariants

These must never be violated.

- Every skill has one canonical `name`
- Every tool belongs to exactly one skill
- `used_by_count` equals the number of unique agent names in `used_by`
- An agent is counted **once per skill**, even if it references multiple tools from that skill
- `enabled = false` does **not** change `used_by_count`
- List and detail routes must agree on `name`, `enabled`, `bundled`, `runtime`, and `used_by_count`
- Missing optional fields never change the meaning of required fields

---

## 5. Required route shapes

### `GET /api/skills`

Returns list-safe fields only. No `used_by` array.

```json
[
  {
    "name": "web_search",
    "description": "Search the web",
    "runtime": "node",
    "installed": true,
    "enabled": true,
    "bundled": true,
    "version": "0.1.0",
    "tool_count": 3,
    "used_by_count": 2
  }
]
```

**Rules:**
- `description` may be empty string
- `version` may be `null` if unknown
- `tool_count` must be integer
- `used_by_count` must be integer
- No `used_by` array in list payload

---

### `GET /api/skills/[name]`

Returns full detail with `used_by` array.

```json
{
  "name": "web_search",
  "description": "Search the web",
  "runtime": "node",
  "installed": true,
  "enabled": true,
  "bundled": true,
  "version": "0.1.0",
  "source": "bundled",
  "entrypoint": "skills/web_search/index.js",
  "prompt_context": null,
  "tools": [
    {
      "name": "search",
      "description": "Query search providers"
    }
  ],
  "used_by_count": 2,
  "used_by": [
    { "name": "researcher" },
    { "name": "analyst" }
  ]
}
```

**Rules:**
- Include both `used_by_count` and `used_by`
- `used_by` must contain unique agent names
- `tools` must be an array, never `null`
- Use `null` or empty array consistently for missing optional data

---

### `PUT /api/skills/[name]/enabled`

**Request:**
```json
{ "enabled": false }
```

**Response:**
```json
{
  "name": "web_search",
  "enabled": false
}
```

**Rules:**
- Must **not** mutate `used_by`
- Must **not** mutate agent configs
- Must fail with `404` for unknown skill
- Must fail with `400` for invalid payload

---

## 6. Field definitions

| Field | Type | Description |
|---|---|---|
| `name` | `string` | Canonical stable identifier for the skill |
| `description` | `string` | Human-readable summary. Safe for UI display. Not used as an identifier |
| `runtime` | `string` enum | Execution type. Values: `python`, `node`, `wasm`, `prompt-only`, `unknown` |
| `installed` | `boolean` | Presence in system |
| `enabled` | `boolean` | Runtime eligibility for future use |
| `bundled` | `boolean` | Shipped with platform rather than user-added |
| `tool_count` | `integer` | Count of tools exposed by the skill |
| `used_by_count` | `integer` | Count of distinct agents referencing at least one tool from the skill |
| `used_by` | `array` | Unique agent references. Minimum shape: `{ "name": string }` |
| `source` | `string` enum | Origin of skill. Values: `bundled`, `local`, `registry`, `unknown` |
| `entrypoint` | `string \| null` | Runtime file path if applicable |
| `prompt_context` | `string \| null` | Prompt-only context payload if applicable |

---

## 7. UI copy conventions

Internal API field names (`used_by_count`, `used_by`) are kept terse for
compatibility. UI surfaces use more precise copy:

| Context | Label |
|---|---|
| Card badge | `Referenced by 2 agents` |
| Drawer section heading | `Agents referencing this skill` |
| Disable warning | references `N agent` / `N agents` |

> **Why "Referenced by" not "Used by":**  
> "Used by" implies runtime verification. This implementation is config-derived.  
> "Referenced by" is accurate: the agent config references a tool provided by this skill.

---

## 8. The one sentence that prevents drift

> **An agent is counted as using a skill when it references one or more tools exposed by that skill.**

If this sentence is absent from context, reviewers will smuggle in different meanings:
- "used" as in "agent was wired to skill explicitly"
- "used" as in "skill was called at runtime this session"
- "used" as in "agent has skill in an enabled list"

None of those are what this implementation does. The sentence above is the contract.

---

## 9. Tests required

### Unit tests — `buildUsageIndex()`

- One tool maps to one skill counts one agent
- Multiple tools from same skill count that agent **once**
- Two agents referencing same skill count as two distinct agents
- Disabled skill still reports the same `used_by_count`
- Agent with missing or empty `tools` array returns zero count cleanly

### Route contract tests

- List route and detail route agree on `name`, `enabled`, `bundled`, `runtime`, `used_by_count`
- Detail route `used_by` contains unique agent names only
- Toggle route response changes only `enabled`; `used_by_count` is unchanged
- Unknown skill returns `404`
- Toggle with non-boolean `enabled` returns `400`

### UI tests

- Card and drawer show the same `used_by_count`
- Disabling a skill with `used_by_count > 0` shows warning banner
- Label reads "Referenced by" (not "Used by")
- Toggle failure rolls back optimistic state to pre-toggle value

---

## 10. Before Phase 3 checklist

- [x] Contract written in `docs/skill-state-contract.md`
- [ ] Link added from Phase 2 PR description
- [x] Semantic rule comment in `buildUsageIndex()` in `lib/skill-usage.js`
- [ ] Unit tests for `buildUsageIndex()` edge cases (see section 9)
- [ ] Route contract tests (see section 9)
