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

## 3. Initialize the Runtime Home

```bash
target/release/openfang init
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

## 6. Verify the Daemon

```bash
curl -s http://127.0.0.1:4200/api/health
target/release/openfang status
target/release/openfang doctor
```

## 7. Spawn a Template Agent

This repository currently includes 30 agent templates under `agents/`.

```bash
target/release/openfang agent spawn agents/hello-world/agent.toml
target/release/openfang agent list
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
