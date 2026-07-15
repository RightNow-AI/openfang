#!/bin/sh
# OpenFang container entrypoint.
#
# Seeds baked agents and a default config onto the data volume on first boot,
# then execs the daemon. Seeding never overwrites files that already exist on
# the volume. Failures are logged (not silently swallowed) so a misconfigured
# volume — wrong permissions, full disk — is diagnosable instead of producing a
# daemon that mysteriously comes up with no agents or config.

set -eu

DATA_DIR="${OPENFANG_HOME:-/data}"
BAKED_AGENTS="/opt/openfang/agents"
BAKED_CONFIG="/opt/openfang/config.toml"

log() { echo "[entrypoint] $*"; }
warn() { echo "[entrypoint] WARNING: $*" >&2; }

log "starting; data dir: $DATA_DIR"

# Seed baked agents (cp -n: never clobber agents already on the volume).
if ! mkdir -p "$DATA_DIR/agents"; then
    warn "could not create $DATA_DIR/agents — agent seeding skipped"
elif [ -d "$BAKED_AGENTS" ]; then
    if cp -rn "$BAKED_AGENTS/." "$DATA_DIR/agents/"; then
        log "seeded baked agents into $DATA_DIR/agents"
    else
        warn "failed to seed baked agents into $DATA_DIR/agents (continuing)"
    fi
else
    log "no baked agents at $BAKED_AGENTS — skipping agent seed"
fi

# Seed the default config only when the volume has none.
if [ -f "$DATA_DIR/config.toml" ]; then
    log "existing config at $DATA_DIR/config.toml — leaving as-is"
elif [ -f "$BAKED_CONFIG" ]; then
    if cp "$BAKED_CONFIG" "$DATA_DIR/config.toml"; then
        log "seeded default config to $DATA_DIR/config.toml"
    else
        warn "failed to seed default config to $DATA_DIR/config.toml (continuing)"
    fi
else
    log "no baked config at $BAKED_CONFIG — skipping config seed"
fi

log "exec: openfang $*"
exec openfang "$@"
