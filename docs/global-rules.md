# Global Rules (`RULES.md`)

A per-user authoritative overlay that lets you steer every agent in your OpenFang instance without editing agent manifests, channel configs, or the daemon binary. Drop a Markdown file at `~/.openfang/RULES.md`, edit freely, and your changes land on the very next message.

---

## Table of Contents

- [What It Is](#what-it-is)
- [Where It Lives](#where-it-lives)
- [Contract](#contract)
- [Position in the System Prompt](#position-in-the-system-prompt)
- [Subagent Inheritance](#subagent-inheritance)
- [Authority](#authority)
- [Getting Started](#getting-started)
- [Worked Example](#worked-example)
- [Gotchas](#gotchas)

---

## What It Is

`RULES.md` is a single Markdown file you control. Its contents are injected verbatim into every agent's system prompt on every turn, framed as authoritative user-level guidance.

It is **not** a config file, **not** a script, and **not** scoped to a single agent. Think of it as the operating manual for your fleet — the place to capture preferences, etiquette, escalation paths, and any standing instructions you would otherwise re-type every conversation.

## Where It Lives

```
~/.openfang/RULES.md
```

(On systems where `OPENFANG_HOME` is set, e.g. inside the Docker image, the file lives at `$OPENFANG_HOME/RULES.md`.)

It sits alongside the other top-level pieces of your OpenFang home:

```
~/.openfang/
├── RULES.md          ← you are here
├── config.toml
├── agents/
├── workspaces/
└── data/
```

## Contract

- **Re-read every turn.** The file is loaded fresh on each model invocation; you do not need to restart the daemon for edits to take effect. Save the file, send your next message, and the new rules are in play.
- **Truncated at load time.** Contents are capped at **2,000 characters** before injection. Anything past the cap is silently dropped. Keep `RULES.md` tight — it is overlay, not encyclopedia.
- **Elided when empty.** If the file is missing, empty, or whitespace-only, the entire section is omitted from the prompt. No empty header, no placeholder.
- **Trimmed.** Leading and trailing whitespace are stripped before injection.

## Position in the System Prompt

The rules are injected as a labelled section sitting between the per-agent workspace context and the live runtime context:

```
…
14. Workspace Context        (the agent's own files)
14.5 Global User Rules        ← RULES.md
15. Live Context              (date, user, peers, memory)
…
```

This placement means the rules see every preceding instruction (identity, persona, skills, workspace) and can countermand them, but they cannot countermand the live runtime block — useful for keeping things like the current date authoritative.

## Subagent Inheritance

Subagents spawned in the same workspace inherit the same `RULES.md`. There is no per-subagent override — a single source of truth for the whole tree.

## Authority

Within the prompt, `RULES.md` is framed as authoritative over earlier sections (persona, agent-specific guidelines, default behaviors) **with one carve-out**: the Safety block always wins. You cannot use `RULES.md` to disable safety guardrails, override irreversible-action confirmation, or bypass the agent's refusal posture.

In practice this means:

- ✅ Style and tone preferences
- ✅ Channel etiquette
- ✅ Escalation rules ("ask before X", "always confirm Y")
- ✅ Defaults ("prefer Z when ambiguous")
- ❌ Disabling safety confirmations
- ❌ Bypassing audit or logging behavior

## Getting Started

A starter template ships with the repository:

```bash
cp docs/templates/RULES.md.example ~/.openfang/RULES.md
$EDITOR ~/.openfang/RULES.md
```

Trim it to what you actually want — the template is deliberately verbose so you can see what shapes the rules can take. Anything you leave commented out has no effect.

## Worked Example

A short, real-world `RULES.md`:

```markdown
# My Rules

## Style
- Be concise. Lead with the answer.
- No filler ("Great question!", "I'd be happy to help!").

## Channel etiquette
- In `#code-system`, keep messages under ~10 lines unless I ask for detail.
- Use code fences for any path or command.

## Escalation
- Never push to `origin/main` without explicit confirmation.
- Confirm before any `rm -rf`, `git reset --hard`, or force-push.
```

That's it. Save, send a message, and every agent in the fleet picks it up on the next turn.

## Gotchas

- **2,000-character cap is per-file, not per-section.** If you blow past it, the *tail* of the file is dropped. Put the most important rules first.
- **No templating.** `RULES.md` is plain Markdown — no variable substitution, no includes, no conditionals.
- **No per-agent scoping.** If you need agent-specific rules, put them in the agent manifest or workspace files, not here.
- **Edits are immediate but not retroactive.** In-flight turns finish with the prompt they started with; the new contents apply from the next turn onward.

---

For the implementation details (loader, prompt builder, kernel wiring), see the source in `crates/openfang-types/src/config.rs` (`read_global_rules`) and `crates/openfang-runtime/src/prompt_builder.rs` (`build_rules_section`).
