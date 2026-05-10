#!/usr/bin/env bash
set -euo pipefail

OPENFANG_HOME="${HOME}/.openfang"
REPO="${HOME}/Projects/openfang"

echo "Deploying DoVi agents..."
mkdir -p "${OPENFANG_HOME}/agents/dovi" "${OPENFANG_HOME}/agents/dovi-feedback-reviewer"
cp "${REPO}/dovi/agents/dovi/agent.toml" "${OPENFANG_HOME}/agents/dovi/agent.toml"
cp "${REPO}/dovi/agents/dovi-feedback-reviewer/agent.toml" "${OPENFANG_HOME}/agents/dovi-feedback-reviewer/agent.toml"

echo "Deploying DoVi skills..."
for skill in planning-system task-tracking session-feedback; do
  mkdir -p "${OPENFANG_HOME}/skills/${skill}"
  cp "${REPO}/dovi/skills/${skill}/SKILL.md" "${OPENFANG_HOME}/skills/${skill}/SKILL.md"
done

echo "Deploying OpenFang binary..."
if [ -f "${REPO}/target/aarch64-unknown-linux-gnu/release/openfang" ]; then
  cp "${REPO}/target/aarch64-unknown-linux-gnu/release/openfang" "${OPENFANG_HOME}/bin/openfang"
  echo "Binary updated from cross-build."
else
  echo "No cross-build binary found. Run 'cargo build --release' or download from CI."
fi

echo "Restarting OpenFang..."
systemctl --user restart openfang.service
echo "Done."
