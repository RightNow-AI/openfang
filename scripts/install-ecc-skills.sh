#!/usr/bin/env bash
# Install ECC-adapted skills into ~/.omtae/skills/ for the OMTAE daemon.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ECC_SRC="${REPO_ROOT}/crates/openfang-skills/ecc"
OMTAE_HOME="${OMTAE_HOME:-${HOME}/.omtae}"
DEST="${OMTAE_HOME}/skills"

if [[ ! -d "${ECC_SRC}" ]]; then
  echo "ECC skill source not found: ${ECC_SRC}" >&2
  exit 1
fi

mkdir -p "${DEST}"

for skill_dir in "${ECC_SRC}"/*/; do
  name="$(basename "${skill_dir}")"
  target="${DEST}/${name}"
  mkdir -p "${target}"
  cp -f "${skill_dir}/SKILL.md" "${target}/SKILL.md"
  echo "Installed ${name} -> ${target}/SKILL.md"
done

echo ""
echo "ECC skills installed to ${DEST}"
echo "Restart omtae-daemon to reload skills, or POST /api/skills/install for hot reload."
