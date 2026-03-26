# Getting Started

This guide is the shortest reliable path for a maintainer to boot the repository from source.

If you need Docker or server deployment instead, start with [deployment.md](deployment.md).

## 1. Prepare the Repository

```bash
git submodule update --init --recursive
```

## 2. Build the CLI Binary

```bash
cargo build --release -p openfang-cli
```

If you are actively iterating on code and want the fastest rebuild/restart loop,
use the debug binary instead:

```bash
cargo build -p openfang-cli
```

On a maintainer machine, `target/debug/openfang` is usually the fastest way to
run the latest local code. Rebuilding a Docker image still recompiles the code,
so it is typically slower than the local debug daemon path on macOS.

## 3. Initialize the Runtime Home

```bash
target/release/openfang init
cp openfang.toml.example ~/.openfang/config.toml
```

If you only built the debug binary, `target/debug/openfang init` is equivalent.

Debug-binary equivalent:

```bash
target/debug/openfang init
cp openfang.toml.example ~/.openfang/config.toml
```

This gives you the standard runtime home:

```text
~/.openfang/
  config.toml
  .env
  data/
```

## 4. Configure a Provider

Edit `~/.openfang/config.toml` or keep the default model from `openfang.toml.example`.

Set at least one real provider key:

```bash
export GROQ_API_KEY=...
```

You can also place it in `~/.openfang/.env`.

## 5. Start the Daemon

```bash
target/release/openfang start
```

For the fastest local development loop, start the debug daemon instead:

```bash
target/debug/openfang start
```

If you are using this fork's integrated `shipinbot` workflow on the same
machine, the preferred local entrypoint is the parent repo stack script:

```bash
scripts/local-stack.sh start
```

For this fork, the fastest full local loop on macOS is usually:

- `scripts/local-stack.sh start` to launch the managed host-host stack
- `scripts/local-stack.sh status` to verify health and single-instance state

Do not default to `docker compose up --build` or a separate
`~/shipinbot-runtime` copy unless you are explicitly testing those topologies.

## 6. Verify the Daemon

```bash
curl -s http://127.0.0.1:4200/api/health
target/release/openfang status
target/release/openfang doctor
```

Debug-binary equivalent:

```bash
target/debug/openfang status
target/debug/openfang doctor
```

## 7. Spawn a Template Agent

This repository currently includes 30 agent templates under `agents/`.

```bash
target/release/openfang agent spawn agents/hello-world/agent.toml
target/release/openfang agent list
```

Debug-binary equivalent:

```bash
target/debug/openfang agent spawn agents/hello-world/agent.toml
target/debug/openfang agent list
```

## 8. Send a Test Message

```bash
curl -s http://127.0.0.1:4200/api/agents
```

Pick an agent ID, then:

```bash
curl -s -X POST "http://127.0.0.1:4200/api/agents/<id>/message" \
  -H "Content-Type: application/json" \
  -d '{"message":"Say hello in five words."}'
```

## 9. Open the Dashboard

Visit:

```text
http://127.0.0.1:4200/
```

## 10. Next Documents

After first boot, continue here:

1. [configuration.md](configuration.md)
2. [architecture.md](architecture.md)
3. [core-modules.md](core-modules.md)
4. [deployment.md](deployment.md)
5. [operations-runbook.md](operations-runbook.md)

If you are working on Telegram media flows or shipinbot integration, continue with the Telegram docs and `projects/shipinbot/docs/`.

## Iteration Loop

For repeated local edits on the same machine, the shortest restart loop is:

```bash
target/debug/openfang stop
cargo build -p openfang-cli
target/debug/openfang start
```

If `projects/shipinbot` is part of the same local task, restart its service
from the same checkout instead of switching to Docker or a runtime copy:

```bash
cd projects/shipinbot
./scripts/start_media_web.sh
```

Use `target/release/openfang` when you need release-like behavior, installation,
or final verification before shipping.
