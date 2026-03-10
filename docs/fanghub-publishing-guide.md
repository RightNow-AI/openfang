# FangHub Publishing Guide

This guide covers how to package and publish your own Hands to the FangHub marketplace.

## 1. Prerequisites

- `fang-cli` installed (`cargo install --path crates/fang-cli`)
- A GitHub account (for `fang login`)
- GPG key set up and configured with Git

## 2. The `HAND.toml` Manifest

Every Hand requires a `HAND.toml` manifest file. This file contains all the metadata for your Hand, including its ID, name, description, category, and agent configuration.

```toml
# Unique identifier for your Hand. Lowercase, alphanumeric, dashes allowed.
id = "my-awesome-hand"

# Human-readable name.
name = "My Awesome Hand"

# A short, one-sentence description of what your Hand does.
description = "This Hand connects to the NASA API to provide daily astronomy pictures."

# Category for discovery in the marketplace.
# Must be one of: "Productivity", "Development", "Data", "Utilities", "Fun"
category = "Data"

# A single emoji to represent your Hand.
icon = "🔭"

# Agent configuration.
[agent]
name = "NASA Bot"
system_prompt = "You are an expert on astronomy. You have access to the NASA API."
model = "gpt-4.1-mini"
```

## 3. The `SKILL.md` File

If your Hand requires specific instructions, API keys, or detailed usage information, include a `SKILL.md` file. This file will be displayed on the Hand's page in the marketplace.

```markdown
# My Awesome Hand Skill

This Hand uses the NASA API. To use it, you need a free API key from [api.nasa.gov](https://api.nasa.gov).

## Setup

1. Get your API key from the NASA website.
2. When you activate this Hand, it will ask for your `NASA_API_KEY`.
3. Paste your key into the settings field.
```

## 4. Packaging Your Hand

Once you have your `HAND.toml` and optional `SKILL.md`, you can package them into a distributable archive using the `fang` CLI.

```bash
# Navigate to the directory containing your HAND.toml and SKILL.md
cd /path/to/my-awesome-hand

# Run the package command
fang package
```

This will create a `my-awesome-hand-1.0.0.tar.gz` file in your directory (assuming the version in your `HAND.toml` is `1.0.0`).

## 5. Publishing Your Hand

To publish your Hand to the marketplace, you first need to log in with your GitHub account.

```bash
fang login
```

This will open a browser window to authorize the FangHub application. Once logged in, you can publish your packaged Hand.

```bash
fang publish my-awesome-hand-1.0.0.tar.gz
```

Your Hand is now live on the FangHub marketplace!
